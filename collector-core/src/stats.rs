use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn clickhouse_url() -> String {
    std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string())
}

async fn ch_query(sql: &str) -> Result<serde_json::Value, StatusCode> {
    let client = reqwest::Client::new();
    let resp = client
        .post(clickhouse_url())
        .query(&[("user", "default")])
        .body(sql.to_string())
        .send()
        .await
        .map_err(|e| {
            tracing::error!("ClickHouse request failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        tracing::error!("ClickHouse error: {body}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    resp.json::<serde_json::Value>().await.map_err(|e| {
        tracing::error!("ClickHouse parse error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

// ─── 2.0 Protocol stats ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ProtocolStatsQuery {
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
    pub minutes: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ProtocolStats {
    pub tcp: u64,
    pub udp: u64,
    pub icmp: u64,
    pub other: u64,
}

pub async fn get_protocol_stats_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<ProtocolStatsQuery>,
) -> Result<Json<ProtocolStats>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).min(1440);
    let device = device_cond(
        &state.db,
        params.exporter_ip.as_deref(),
        params.role.as_deref(),
    )
    .await;
    let wc = where_clause(&device, minutes, "MINUTE");

    let sql = format!(
        "SELECT protocol, sum(bytes) AS total_bytes \
         FROM network_flows_v4 {wc} GROUP BY protocol \
         UNION ALL \
         SELECT protocol, sum(bytes) AS total_bytes \
         FROM network_flows_v6 {wc} GROUP BY protocol \
         FORMAT JSON"
    );

    let val = ch_query(&sql).await?;
    let rows = val["data"].as_array().cloned().unwrap_or_default();

    let mut tcp = 0u64;
    let mut udp = 0u64;
    let mut icmp = 0u64;
    let mut other = 0u64;

    for row in rows {
        let protocol = row["protocol"].as_u64().unwrap_or(0) as u8;
        let bytes = row["total_bytes"]
            .as_str()
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);
        match protocol {
            6 => tcp += bytes,
            17 => udp += bytes,
            1 => icmp += bytes,
            _ => other += bytes,
        }
    }

    Ok(Json(ProtocolStats {
        tcp,
        udp,
        icmp,
        other,
    }))
}

// ─── 2.1 Top talkers ─────────────────────────────────────────────────────────

/// Gate de licença das views analíticas (task 14.1): degradado → 402;
/// senão devolve o teto de linhas permitido pelo max_talkers.
fn license_gate(
    state: &crate::auth::AppState,
    requested_limit: u32,
) -> Result<(u32, bool), StatusCode> {
    let lic = state.license.read().unwrap();
    if lic.degraded {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    match lic.max_talkers {
        Some(t) if t < requested_limit => Ok((t, true)),
        _ => Ok((requested_limit, false)),
    }
}

#[derive(Debug, Deserialize)]
pub struct TopTalkersQuery {
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
    pub minutes: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopTalkerRow {
    pub src_ip: String,
    pub total_bytes: u64,
    pub total_packets: u64,
    pub flow_count: u64,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub unknown_bytes: u64,
    pub p95_bps: u64,
    pub avg_bps: u64,
    /// cgnat | internal | internet (task 17.7)
    pub ip_class: &'static str,
}

pub async fn get_top_talkers_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<TopTalkersQuery>,
) -> Result<Json<Vec<TopTalkerRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).clamp(1, 1440);
    let requested = params.limit.unwrap_or(20).min(100);
    let (limit, capped) = license_gate(&state, requested)?;
    let device = device_cond(
        &state.db,
        params.exporter_ip.as_deref(),
        params.role.as_deref(),
    )
    .await;
    let wc = complete_minutes_where(&device, minutes);
    let p95 = p95_bps_expr("b", minutes);

    // Per-minute sums first so the p95 sees whole buckets; an IP lives in one
    // family only, so each table can rank on its own
    let leg = |table: &str| {
        format!(
            "SELECT toString(src_ip) AS src_ip, sum(b) AS total_bytes, sum(p) AS total_packets, \
                    sum(f) AS flow_count, sum(ib) AS in_bytes, sum(ob) AS out_bytes, \
                    sum(ub) AS unknown_bytes, {p95} AS p95_bps, max(sa) AS src_asn FROM (\
               SELECT src_ip, toStartOfMinute(timestamp) AS minute, sum(bytes) AS b, \
                      sum(packets) AS p, sum(flow_count) AS f, \
                      sumIf(bytes, direction = 0) AS ib, sumIf(bytes, direction = 1) AS ob, \
                      sumIf(bytes, direction = 255) AS ub, any(src_asn) AS sa \
               FROM {table} {wc} GROUP BY src_ip, minute\
             ) GROUP BY src_ip ORDER BY p95_bps DESC, total_bytes DESC LIMIT {limit}"
        )
    };
    // IPv4 e IPv6 nativos não têm supertipo no UNION ALL: as duas pernas viram String
    let sql = format!(
        "{} UNION ALL {} FORMAT JSON",
        leg("network_flows_v4"),
        leg("network_flows_v6")
    );

    let val = ch_query(&sql).await?;
    let asns: HashMap<String, u32> = val["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| {
                    let ip = r["src_ip"].as_str().unwrap_or_default().to_string();
                    (ip, parse_u64_field(&r["src_asn"]) as u32)
                })
                .collect()
        })
        .unwrap_or_default();
    let classifier = crate::netclass::NetClassifier::load(&state.db).await;
    let mut rows: Vec<TopTalkerRow> = parse_rows::<TopTalkerRowRaw>(&val);
    for r in &mut rows {
        r.avg_bps = avg_bps(r.total_bytes, minutes);
        r.ip_class = classifier.classify_str(&r.src_ip, asns.get(&r.src_ip).copied().unwrap_or(0));
    }
    rows.sort_by(|a, b| {
        b.p95_bps
            .cmp(&a.p95_bps)
            .then(b.total_bytes.cmp(&a.total_bytes))
    });
    rows.truncate(limit as usize);

    // Fora da licença: agrega o restante como "Outros" — o dado existe,
    // o detalhe é o que a licença libera
    if capped {
        let total_sql = format!(
            "SELECT sum(b) FROM ( \
               SELECT sum(bytes) AS b FROM network_flows_v4 {wc} \
               UNION ALL SELECT sum(bytes) AS b FROM network_flows_v6 {wc})"
        );
        if let Ok(val) = ch_query(&format!("{total_sql} FORMAT JSON")).await {
            let grand: u64 = val["data"][0]
                .as_object()
                .and_then(|o| o.values().next())
                .map(parse_u64_field)
                .unwrap_or(0);
            let shown: u64 = rows.iter().map(|r| r.total_bytes).sum();
            if grand > shown {
                rows.push(TopTalkerRow {
                    src_ip: "outros".to_string(),
                    total_bytes: grand - shown,
                    total_packets: 0,
                    flow_count: 0,
                    in_bytes: 0,
                    out_bytes: 0,
                    unknown_bytes: 0,
                    p95_bps: 0,
                    avg_bps: avg_bps(grand - shown, minutes),
                    ip_class: "internet",
                });
            }
        }
    }
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct TopTalkerRowRaw {
    src_ip: String,
    total_bytes: StringOrU64,
    total_packets: StringOrU64,
    flow_count: StringOrU64,
    in_bytes: StringOrU64,
    out_bytes: StringOrU64,
    unknown_bytes: StringOrU64,
    p95_bps: StringOrU64,
}

impl From<TopTalkerRowRaw> for TopTalkerRow {
    fn from(r: TopTalkerRowRaw) -> Self {
        TopTalkerRow {
            src_ip: r.src_ip,
            total_bytes: r.total_bytes.into(),
            total_packets: r.total_packets.into(),
            flow_count: r.flow_count.into(),
            in_bytes: r.in_bytes.into(),
            out_bytes: r.out_bytes.into(),
            unknown_bytes: r.unknown_bytes.into(),
            p95_bps: r.p95_bps.into(),
            avg_bps: 0,
            ip_class: "internet",
        }
    }
}

// ─── 2.2 ASN traffic ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AsnQuery {
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
    pub minutes: Option<u32>,
    pub limit: Option<u32>,
    pub direction: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AsnRow {
    pub asn: u64,
    pub label: String,
    /// nearest-rank p95 of zero-filled 1-minute buckets over the window (bps)
    pub p95_bps: u64,
    pub avg_bps: u64,
    pub total_bytes: u64,
    pub total_packets: u64,
}

pub async fn get_asn_stats_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<AsnQuery>,
) -> Result<Json<Vec<AsnRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(60).clamp(1, 1440);
    let requested = params.limit.unwrap_or(20).min(100);
    let (limit, _) = license_gate(&state, requested)?;
    let direction = params.direction.as_deref().unwrap_or("both");

    let device = device_cond(
        &state.db,
        params.exporter_ip.as_deref(),
        params.role.as_deref(),
    )
    .await;
    let sql = build_asn_query(&device, minutes, limit, direction);
    let val = ch_query(&sql).await?;

    let rows = val["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            let asn = parse_u64_field(&row["asn"]);
            let total_bytes = parse_u64_field(&row["total_bytes"]);
            AsnRow {
                asn,
                label: if asn == 0 {
                    "Unknown".to_string()
                } else {
                    format!("AS{asn}")
                },
                p95_bps: parse_u64_field(&row["p95_bps"]),
                avg_bps: avg_bps(total_bytes, minutes),
                total_bytes,
                total_packets: parse_u64_field(&row["total_packets"]),
            }
        })
        .collect();

    Ok(Json(rows))
}

/// 1-based position, among buckets sorted descending, of the nearest-rank p95
/// of `buckets` values. Buckets where the ASN was silent are zeros at the
/// bottom, so only the non-zero ones need to be sorted.
fn p95_desc_position(buckets: u32) -> u32 {
    let rank = (buckets as f64 * 0.95).ceil() as u32;
    buckets - rank + 1
}

fn build_asn_query(device: &str, minutes: u32, limit: u32, direction: &str) -> String {
    let filter = complete_minutes_where(device, minutes);
    let side = |table: &str, col: &str| {
        format!(
            "SELECT {col} AS asn, toStartOfMinute(timestamp) AS minute, bytes, packets \
             FROM {table} {filter} AND {col} != 0"
        )
    };
    let cols: &[&str] = match direction {
        "src" => &["src_asn"],
        "dst" => &["dst_asn"],
        _ => &["src_asn", "dst_asn"],
    };
    let sources = ["network_flows_v4", "network_flows_v6"]
        .iter()
        .flat_map(|t| cols.iter().map(move |c| side(t, c)))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    let p95 = p95_bps_expr("b", minutes);
    format!(
        "SELECT asn, sum(b) AS total_bytes, sum(p) AS total_packets, {p95} AS p95_bps \
         FROM (\
           SELECT asn, minute, sum(bytes) AS b, sum(packets) AS p \
           FROM ({sources}) GROUP BY asn, minute\
         ) GROUP BY asn ORDER BY p95_bps DESC, total_bytes DESC LIMIT {limit} FORMAT JSON"
    )
}

#[cfg(test)]
mod asn_tests {
    use super::p95_desc_position;

    #[test]
    fn p95_position_matches_nearest_rank() {
        // 60 buckets: rank 57 ascending = 4th largest
        assert_eq!(p95_desc_position(60), 4);
        // 15 buckets: rank 15 ascending = the max
        assert_eq!(p95_desc_position(15), 1);
        assert_eq!(p95_desc_position(1440), 73);
        assert_eq!(p95_desc_position(1), 1);
    }
}

// ─── 2.3 Port breakdown ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PortBreakdownQuery {
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
    pub minutes: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PortRow {
    /// Service port: lower of src/dst port, so both directions of a
    /// conversation land on the same row (task 17.2 R-04)
    pub port: u16,
    pub service: String,
    pub p95_bps: u64,
    pub avg_bps: u64,
    pub total_bytes: u64,
    pub total_packets: u64,
    /// Share of the window's total volume, not of the rows shown
    pub share_pct: f64,
}

fn port_to_service(port: u16) -> &'static str {
    match port {
        0 => "Sem porta",
        20 | 21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        67 | 68 => "DHCP",
        80 => "HTTP",
        110 => "POP3",
        123 => "NTP",
        143 => "IMAP",
        161 => "SNMP",
        179 => "BGP",
        389 => "LDAP",
        443 => "HTTPS",
        465 => "SMTPS",
        514 => "Syslog",
        587 => "SMTP/TLS",
        636 => "LDAPS",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "MSSQL",
        1521 => "Oracle",
        3306 => "MySQL",
        3389 => "RDP",
        5432 => "PostgreSQL",
        5900 => "VNC",
        6379 => "Redis",
        8080 => "HTTP-Alt",
        8443 => "HTTPS-Alt",
        9200 => "Elasticsearch",
        27017 => "MongoDB",
        _ => "Other",
    }
}

pub async fn get_port_breakdown_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<PortBreakdownQuery>,
) -> Result<Json<Vec<PortRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).clamp(1, 1440);
    let requested = params.limit.unwrap_or(20).min(100);
    let (limit, _) = license_gate(&state, requested)?;
    let device = device_cond(
        &state.db,
        params.exporter_ip.as_deref(),
        params.role.as_deref(),
    )
    .await;
    let wc = complete_minutes_where(&device, minutes);
    let p95 = p95_bps_expr("b", minutes);

    let side = |table: &str| {
        format!(
            "SELECT least(src_port, dst_port) AS port, toStartOfMinute(timestamp) AS minute, \
                    bytes, packets FROM {table} {wc}"
        )
    };
    let sql = format!(
        "SELECT port, sum(b) AS total_bytes, sum(p) AS total_packets, {p95} AS p95_bps FROM (\
           SELECT port, minute, sum(bytes) AS b, sum(packets) AS p \
           FROM ({} UNION ALL {}) GROUP BY port, minute\
         ) GROUP BY port ORDER BY p95_bps DESC, total_bytes DESC LIMIT {limit} FORMAT JSON",
        side("network_flows_v4"),
        side("network_flows_v6"),
    );
    let total_sql = format!(
        "SELECT (SELECT sum(bytes) FROM network_flows_v4 {wc}) \
              + (SELECT sum(bytes) FROM network_flows_v6 {wc}) AS total FORMAT JSON"
    );

    let (val, total_val) = tokio::try_join!(ch_query(&sql), ch_query(&total_sql))?;
    let window_bytes = parse_u64_field(&total_val["data"][0]["total"]);

    let rows = val["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            let port = parse_u64_field(&row["port"]) as u16;
            let total_bytes = parse_u64_field(&row["total_bytes"]);
            PortRow {
                port,
                service: port_to_service(port).to_string(),
                p95_bps: parse_u64_field(&row["p95_bps"]),
                avg_bps: avg_bps(total_bytes, minutes),
                total_bytes,
                total_packets: parse_u64_field(&row["total_packets"]),
                share_pct: if window_bytes > 0 {
                    total_bytes as f64 / window_bytes as f64 * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect();

    Ok(Json(rows))
}

// ─── 2.4 Traffic timeline ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
    pub hours: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub minute: u64,
    /// in + out + unknown (kept for backward compatibility)
    pub total_bytes: u64,
    pub total_packets: u64,
    /// direction = 0 (ingress) — router perspective, IE 61
    pub in_bytes: u64,
    pub in_packets: u64,
    /// direction = 1 (egress)
    pub out_bytes: u64,
    pub out_packets: u64,
    /// direction = 255 (exporter does not report IE 61)
    pub unknown_bytes: u64,
    pub unknown_packets: u64,
    /// Split por família de IP (task 13.7)
    pub v4_bytes: u64,
    pub v6_bytes: u64,
    /// Família × direção para o gráfico empilhado (task 13.9);
    /// sem direção (255) conta como entrada
    pub v4_in_bytes: u64,
    pub v4_out_bytes: u64,
    pub v6_in_bytes: u64,
    pub v6_out_bytes: u64,
}

pub async fn get_timeline_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<TimelineQuery>,
) -> Result<Json<Vec<TimelinePoint>>, StatusCode> {
    // up to 7 days — the dashboard heatmap aggregates 168h client-side
    let hours = params.hours.unwrap_or(1).min(168);
    // minute buckets get heavy past 24h; switch to hourly granularity
    let bucket_fn = if params.hours.unwrap_or(1) > 24 {
        "toStartOfHour"
    } else {
        "toStartOfMinute"
    };
    let device = device_cond(
        &state.db,
        params.exporter_ip.as_deref(),
        params.role.as_deref(),
    )
    .await;
    let wc = where_clause(&device, hours, "HOUR");

    // Single pass per table: sumIf splits by direction without extra scans
    let dir_cols = "sumIf(bytes, direction = 0) AS in_bytes, \
                sumIf(packets, direction = 0) AS in_packets, \
                sumIf(bytes, direction = 1) AS out_bytes, \
                sumIf(packets, direction = 1) AS out_packets, \
                sumIf(bytes, direction = 255) AS unknown_bytes, \
                sumIf(packets, direction = 255) AS unknown_packets";
    // The literal `fam` column tags which table each union arm came from
    let sql = format!(
        "SELECT toUnixTimestamp({bucket_fn}(timestamp)) AS minute, 4 AS fam, {dir_cols} \
         FROM network_flows_v4 {wc} GROUP BY minute ORDER BY minute ASC \
         UNION ALL \
         SELECT toUnixTimestamp({bucket_fn}(timestamp)) AS minute, 6 AS fam, {dir_cols} \
         FROM network_flows_v6 {wc} GROUP BY minute ORDER BY minute ASC \
         FORMAT JSON"
    );

    let val = ch_query(&sql).await?;
    let mut map: BTreeMap<u64, [u64; 12]> = BTreeMap::new();

    for row in val["data"].as_array().cloned().unwrap_or_default() {
        let minute = parse_u64_field(&row["minute"]);
        let e = map.entry(minute).or_insert([0; 12]);
        let in_b = parse_u64_field(&row["in_bytes"]);
        let out_b = parse_u64_field(&row["out_bytes"]);
        let unk_b = parse_u64_field(&row["unknown_bytes"]);
        e[0] += in_b;
        e[1] += parse_u64_field(&row["in_packets"]);
        e[2] += out_b;
        e[3] += parse_u64_field(&row["out_packets"]);
        e[4] += unk_b;
        e[5] += parse_u64_field(&row["unknown_packets"]);
        if parse_u64_field(&row["fam"]) == 6 {
            e[7] += in_b + out_b + unk_b;
            e[10] += in_b + unk_b;
            e[11] += out_b;
        } else {
            e[6] += in_b + out_b + unk_b;
            e[8] += in_b + unk_b;
            e[9] += out_b;
        }
    }

    let points = map
        .into_iter()
        .map(|(minute, d)| TimelinePoint {
            minute,
            total_bytes: d[0] + d[2] + d[4],
            total_packets: d[1] + d[3] + d[5],
            in_bytes: d[0],
            in_packets: d[1],
            out_bytes: d[2],
            out_packets: d[3],
            unknown_bytes: d[4],
            unknown_packets: d[5],
            v4_bytes: d[6],
            v6_bytes: d[7],
            v4_in_bytes: d[8],
            v4_out_bytes: d[9],
            v6_in_bytes: d[10],
            v6_out_bytes: d[11],
        })
        .collect();

    Ok(Json(points))
}

// ─── 2.5 Per-exporter summary ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExporterSummaryQuery {
    pub minutes: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ExporterSummaryRow {
    pub exporter_ip: String,
    pub total_bytes: u64,
    pub flow_count: u64,
    pub unique_sources: u64,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub unknown_bytes: u64,
    /// "in+out" | "in" | "out" | "none" — whether the exporter reports IE 61
    pub direction_mode: String,
    /// borda | bng | cgnat (task 17.9)
    pub role: String,
}

pub async fn get_exporter_summary_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<ExporterSummaryQuery>,
) -> Result<Json<Vec<ExporterSummaryRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).min(1440);
    let time_filter = format!("WHERE timestamp >= now() - INTERVAL {} MINUTE", minutes);

    let cols = "sum(bytes) AS total_bytes, sum(flow_count) AS flow_count, \
                uniq(src_ip) AS unique_sources, \
                sumIf(bytes, direction = 0) AS in_bytes, \
                sumIf(bytes, direction = 1) AS out_bytes, \
                sumIf(bytes, direction = 255) AS unknown_bytes";
    let sql = format!(
        "SELECT toString(exporter_ip) AS exporter_ip, {cols} \
         FROM network_flows_v4 {time_filter} GROUP BY exporter_ip ORDER BY total_bytes DESC \
         UNION ALL \
         SELECT toString(exporter_ip) AS exporter_ip, {cols} \
         FROM network_flows_v6 {time_filter} GROUP BY exporter_ip ORDER BY total_bytes DESC \
         FORMAT JSON"
    );

    let val = ch_query(&sql).await?;
    let mut merged: HashMap<String, ExporterSummaryRow> = HashMap::new();

    for row in val["data"].as_array().cloned().unwrap_or_default() {
        let ip = row["exporter_ip"].as_str().unwrap_or("").to_string();
        let bytes = parse_u64_field(&row["total_bytes"]);
        let flows = parse_u64_field(&row["flow_count"]);
        let uniq = parse_u64_field(&row["unique_sources"]);
        let in_b = parse_u64_field(&row["in_bytes"]);
        let out_b = parse_u64_field(&row["out_bytes"]);
        let unk_b = parse_u64_field(&row["unknown_bytes"]);

        let e = merged.entry(ip.clone()).or_insert(ExporterSummaryRow {
            exporter_ip: ip,
            total_bytes: 0,
            flow_count: 0,
            unique_sources: 0,
            in_bytes: 0,
            out_bytes: 0,
            unknown_bytes: 0,
            direction_mode: String::new(),
            role: String::new(),
        });
        e.total_bytes += bytes;
        e.flow_count += flows;
        e.unique_sources += uniq;
        e.in_bytes += in_b;
        e.out_bytes += out_b;
        e.unknown_bytes += unk_b;
    }

    let roles = crate::exporters::role_map(&state.db).await;
    let mut rows: Vec<ExporterSummaryRow> = merged.into_values().collect();
    for r in rows.iter_mut() {
        r.role = roles
            .get(&r.exporter_ip)
            .cloned()
            .unwrap_or_else(|| crate::exporters::DEFAULT_ROLE.to_string());
        r.direction_mode = match (r.in_bytes > 0, r.out_bytes > 0) {
            (true, true) => "in+out",
            (true, false) => "in",
            (false, true) => "out",
            (false, false) => "none",
        }
        .to_string();
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.total_bytes));
    Ok(Json(rows))
}

// ─── 12.1 NOC overview ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    pub minutes: Option<u32>,
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct SamplingExporter {
    pub exporter_ip: String,
    pub rate: u32,
}

#[derive(Debug, Serialize, Default)]
pub struct Overview {
    /// bps in the last full minute, split by direction (unknown counted as in
    /// so single-direction exporters still fill the headline tile)
    pub current_bps_in: u64,
    pub current_bps_out: u64,
    pub current_pps: u64,
    pub flows_per_sec: u64,
    /// max 1-minute bucket over the last 5 minutes (bps)
    pub peak_bps_5m: u64,
    /// 95th percentile of 1-minute buckets over the window (bps) — billing
    pub p95_bps: u64,
    pub avg_bps: u64,
    pub active_talkers: u64,
    pub active_exporters: u64,
    pub total_bytes_24h: u64,
    pub alerts_24h: u64,
    /// events in the last 60 minutes (there is no ack state on events)
    pub alerts_active: u64,
    pub bgp_sessions_up: u64,
    pub bgp_sessions_total: u64,
    pub top_protocol: String,
    pub sampling_exporters: Vec<SamplingExporter>,
    /// Fatia do IPv6 no volume da janela, 0–100 (task 16.x — tile NOC)
    pub v6_share_pct: f64,
    /// Bytes em disco das tabelas de flow (compressão inclusa)
    pub storage_bytes: u64,
    /// Dias entre o flow mais antigo armazenado e agora
    pub storage_days: u64,
    /// % de pacotes UDP descartados por backpressure (acumulado do processo)
    pub collector_drop_pct: f64,
    /// Maior atraso flowEnd→chegada entre exporters (s)
    pub telemetry_lag_secs: f64,
    /// Alertas por hora, 24 posições (mais antiga → mais recente)
    pub alerts_by_hour: Vec<u64>,
}

pub async fn get_overview_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<OverviewQuery>,
) -> Result<Json<Overview>, StatusCode> {
    let minutes = params.minutes.unwrap_or(60).clamp(1, 1440);
    let mut ov = Overview::default();

    let device = device_cond(
        &state.db,
        params.exporter_ip.as_deref(),
        params.role.as_deref(),
    )
    .await;

    // ── 1-minute buckets over the window (both tables, merged in Rust) ──
    // A coluna literal `fam` permite o share v6 sem uma segunda varredura
    let bucket_sql = format!(
        "SELECT toUnixTimestamp(toStartOfMinute(timestamp)) AS minute, 4 AS fam, \
                sum(bytes) AS b, sum(packets) AS p, sum(flow_count) AS f, \
                sumIf(bytes, direction = 1) AS out_b \
         FROM network_flows_v4 WHERE {device} AND timestamp >= now() - INTERVAL {minutes} MINUTE GROUP BY minute \
         UNION ALL \
         SELECT toUnixTimestamp(toStartOfMinute(timestamp)) AS minute, 6 AS fam, \
                sum(bytes) AS b, sum(packets) AS p, sum(flow_count) AS f, \
                sumIf(bytes, direction = 1) AS out_b \
         FROM network_flows_v6 WHERE {device} AND timestamp >= now() - INTERVAL {minutes} MINUTE GROUP BY minute \
         FORMAT JSON"
    );
    let val = ch_query(&bucket_sql).await?;
    let mut buckets: BTreeMap<u64, [u64; 4]> = BTreeMap::new();
    let mut v6_bytes: u64 = 0;
    let mut all_bytes: u64 = 0;
    for row in val["data"].as_array().cloned().unwrap_or_default() {
        let m = parse_u64_field(&row["minute"]);
        let b = parse_u64_field(&row["b"]);
        let e = buckets.entry(m).or_insert([0; 4]);
        e[0] += b;
        e[1] += parse_u64_field(&row["p"]);
        e[2] += parse_u64_field(&row["f"]);
        e[3] += parse_u64_field(&row["out_b"]);
        all_bytes += b;
        if parse_u64_field(&row["fam"]) == 6 {
            v6_bytes += b;
        }
    }
    if all_bytes > 0 {
        ov.v6_share_pct = (v6_bytes as f64 / all_bytes as f64) * 100.0;
    }

    // ── armazenamento: bytes em disco + idade do dado mais antigo ──
    let storage_sql = "SELECT sum(bytes_on_disk) AS disk, \
                toUnixTimestamp(min(min_time)) AS oldest \
         FROM system.parts \
         WHERE active AND table IN ('network_flows_v4', 'network_flows_v6') \
         FORMAT JSON";
    if let Ok(val) = ch_query(storage_sql).await {
        if let Some(row) = val["data"].as_array().and_then(|a| a.first()) {
            ov.storage_bytes = parse_u64_field(&row["disk"]);
            let oldest = parse_u64_field(&row["oldest"]);
            if oldest > 0 {
                ov.storage_days = (unix_now().saturating_sub(oldest)) / 86_400;
            }
        }
    }

    if !buckets.is_empty() {
        // Last full minute = second-to-last bucket when the last is still open
        let entries: Vec<(&u64, &[u64; 4])> = buckets.iter().collect();
        let now_min = (unix_now() / 60) * 60;
        let current = entries
            .iter()
            .rev()
            .find(|(m, _)| **m < now_min)
            .or_else(|| entries.last())
            .map(|(_, v)| **v)
            .unwrap_or_default();
        ov.current_bps_out = current[3] * 8 / 60;
        ov.current_bps_in = (current[0].saturating_sub(current[3])) * 8 / 60;
        ov.current_pps = current[1] / 60;
        ov.flows_per_sec = current[2] / 60;

        let mut bps: Vec<u64> = buckets.values().map(|v| v[0] * 8 / 60).collect();
        ov.avg_bps = bps.iter().sum::<u64>() / bps.len() as u64;
        bps.sort_unstable();
        // nearest-rank p95
        let idx = ((bps.len() as f64) * 0.95).ceil() as usize;
        ov.p95_bps = bps[idx.saturating_sub(1).min(bps.len() - 1)];
        ov.peak_bps_5m = buckets
            .iter()
            .rev()
            .take(5)
            .map(|(_, v)| v[0] * 8 / 60)
            .max()
            .unwrap_or(0);
    }

    // ── saúde do coletor: perda por backpressure + lag de telemetria ──
    {
        let received = state.metrics.packets_received.get();
        let dropped = state.metrics.packets_dropped.get();
        if received > 0 {
            ov.collector_drop_pct = dropped as f64 / received as f64 * 100.0;
        }
        ov.telemetry_lag_secs = state.metrics.telemetry_lag_max();
    }

    // ── sparkline: alertas por hora nas últimas 24h (SQLite) ──
    {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT CAST((strftime('%s','now') - strftime('%s', created_at)) / 3600 AS INTEGER) \
                    AS hours_ago, COUNT(*) \
             FROM alert_events \
             WHERE created_at >= datetime('now', '-24 hours') \
             GROUP BY hours_ago",
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        let mut spark = vec![0u64; 24];
        for (hours_ago, n) in rows {
            let idx = 23i64.saturating_sub(hours_ago);
            if (0..24).contains(&idx) {
                spark[idx as usize] = n as u64;
            }
        }
        ov.alerts_by_hour = spark;
    }

    // ── active talkers / exporters (last 5 min) ──
    // Talkers follow the device scope; active exporters stay global
    let act_sql = format!(
        "SELECT uniqIf(src_ip, {device}) AS talkers, uniq(exporter_ip) AS exps \
         FROM network_flows_v4 WHERE timestamp >= now() - INTERVAL 5 MINUTE \
         UNION ALL \
         SELECT uniqIf(src_ip, {device}) AS talkers, uniq(exporter_ip) AS exps \
         FROM network_flows_v6 WHERE timestamp >= now() - INTERVAL 5 MINUTE \
         FORMAT JSON"
    );
    if let Ok(val) = ch_query(&act_sql).await {
        for row in val["data"].as_array().cloned().unwrap_or_default() {
            ov.active_talkers += parse_u64_field(&row["talkers"]);
            ov.active_exporters = ov.active_exporters.max(parse_u64_field(&row["exps"]));
        }
    }

    // ── 24h volume + top protocol (single scan) ──
    let day_sql = format!(
        "SELECT sum(bytes) AS b, \
                sumIf(bytes, protocol = 6) AS tcp, sumIf(bytes, protocol = 17) AS udp, \
                sumIf(bytes, protocol = 1) AS icmp \
         FROM network_flows_v4 WHERE {device} AND timestamp >= now() - INTERVAL 24 HOUR \
         UNION ALL \
         SELECT sum(bytes) AS b, \
                sumIf(bytes, protocol = 6) AS tcp, sumIf(bytes, protocol = 17) AS udp, \
                sumIf(bytes, protocol = 1) AS icmp \
         FROM network_flows_v6 WHERE {device} AND timestamp >= now() - INTERVAL 24 HOUR \
         FORMAT JSON"
    );
    if let Ok(val) = ch_query(&day_sql).await {
        let (mut tcp, mut udp, mut icmp) = (0u64, 0u64, 0u64);
        for row in val["data"].as_array().cloned().unwrap_or_default() {
            ov.total_bytes_24h += parse_u64_field(&row["b"]);
            tcp += parse_u64_field(&row["tcp"]);
            udp += parse_u64_field(&row["udp"]);
            icmp += parse_u64_field(&row["icmp"]);
        }
        let other = ov.total_bytes_24h.saturating_sub(tcp + udp + icmp);
        ov.top_protocol = [
            ("TCP", tcp),
            ("UDP", udp),
            ("ICMP", icmp),
            ("Outros", other),
        ]
        .iter()
        .max_by_key(|(_, v)| *v)
        .map(|(n, _)| n.to_string())
        .unwrap_or_default();
    }

    // ── alerts (SQLite) ──
    if let Ok(row) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT \
           (SELECT COUNT(*) FROM alert_events WHERE created_at >= datetime('now','-24 hours')), \
           (SELECT COUNT(*) FROM alert_events WHERE created_at >= datetime('now','-60 minutes'))",
    )
    .fetch_one(&state.db)
    .await
    {
        ov.alerts_24h = row.0 as u64;
        ov.alerts_active = row.1 as u64;
    }

    // ── BGP sessions (in-memory map) ──
    if let Ok(sessions) = state.bgp_sessions.read() {
        ov.bgp_sessions_total = sessions.len() as u64;
        ov.bgp_sessions_up = sessions.values().filter(|s| s.state == "up").count() as u64;
    }

    // ── sampled exporters (from the Prometheus gauge) ──
    for family in state.metrics.registry.gather() {
        if family.name() != "exporter_sampling_rate" {
            continue;
        }
        for metric in family.get_metric() {
            let rate = metric.get_gauge().value() as u32;
            if rate <= 1 {
                continue;
            }
            let ip = metric
                .get_label()
                .iter()
                .find(|l| l.name() == "exporter_ip")
                .map(|l| l.value().to_string())
                .unwrap_or_default();
            ov.sampling_exporters.push(SamplingExporter {
                exporter_ip: ip,
                rate,
            });
        }
    }

    Ok(Json(ov))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── 17.1 Talker detail ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TalkerQuery {
    pub ip: String,
    pub minutes: Option<u32>,
    pub exporter_ip: Option<String>,
    /// borda | bng | cgnat — used when exporter_ip is absent (task 17.9)
    pub role: Option<String>,
    /// all | internet | internal — filters by the peer's class (task 17.7)
    pub scope: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TalkerPoint {
    pub t: u64,
    pub up_bps: u64,
    pub down_bps: u64,
}

#[derive(Debug, Serialize)]
pub struct TalkerConversation {
    pub peer: String,
    pub peer_asn: u64,
    pub peer_class: &'static str,
    /// Other side of the talker's NAT translation; empty without NAT (task 17.8)
    pub translated: String,
    pub protocol: u64,
    pub port: u64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub packets: u64,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Serialize)]
pub struct TalkerPort {
    pub protocol: u64,
    pub port: u64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub peers: u64,
}

#[derive(Debug, Serialize)]
pub struct RoleVolume {
    pub role: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct TalkerDetail {
    pub ip: String,
    pub ip_class: &'static str,
    pub scope: &'static str,
    /// Role whose exporters were used (task 17.9 R-04)
    pub role: String,
    /// Every role the IP shows up in, with its volume in the window
    pub seen_on: Vec<RoleVolume>,
    pub bucket_secs: u64,
    pub from: u64,
    pub to: u64,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub up_p95_bps: u64,
    pub down_p95_bps: u64,
    pub series: Vec<TalkerPoint>,
    pub conversations: Vec<TalkerConversation>,
    pub ports: Vec<TalkerPort>,
}

pub async fn get_talker_handler(
    State(state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<TalkerQuery>,
) -> Result<Json<TalkerDetail>, StatusCode> {
    license_gate(&state, 1)?;
    let ip: std::net::IpAddr = params.ip.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let minutes = params.minutes.unwrap_or(5).clamp(1, 1440);
    let bucket_secs: u64 = if minutes <= 15 { 1 } else { 60 };

    let (table, x, zero) = match ip {
        std::net::IpAddr::V4(v4) => (
            "network_flows_v4",
            format!("toIPv4('{v4}')"),
            "toIPv4('0.0.0.0')",
        ),
        std::net::IpAddr::V6(v6) => (
            "network_flows_v6",
            format!("toIPv6('{v6}')"),
            "toIPv6('::')",
        ),
    };
    // NAT-aware matching (task 17.8 R-04): the IP may sit on either side of
    // the translation when the exporter is the NAT device
    let is_up = format!("(src_ip = {x} OR nat_src_ip = {x})");
    let is_down = format!("(dst_ip = {x} OR nat_dst_ip = {x})");
    let roles = crate::exporters::role_map(&state.db).await;
    let role_of = |exporter: &str| {
        roles
            .get(exporter)
            .cloned()
            .unwrap_or_else(|| crate::exporters::DEFAULT_ROLE.to_string())
    };

    // Where the IP shows up, per role — never summed across roles (R-03/R-04)
    let seen_sql = format!(
        "SELECT toString(exporter_ip) AS exporter, sum(bytes) AS b FROM {table} \
         WHERE timestamp >= now() - INTERVAL {minutes} MINUTE \
           AND (src_ip = {x} OR dst_ip = {x} OR nat_src_ip = {x} OR nat_dst_ip = {x}) \
         GROUP BY exporter FORMAT JSON"
    );
    let mut per_role: HashMap<String, u64> = HashMap::new();
    for r in ch_query(&seen_sql).await?["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        *per_role
            .entry(role_of(r["exporter"].as_str().unwrap_or_default()))
            .or_default() += parse_u64_field(&r["b"]);
    }
    let mut seen_on: Vec<RoleVolume> = per_role
        .into_iter()
        .map(|(role, bytes)| RoleVolume { role, bytes })
        .collect();
    // Ties go to the border view, then BNG, then CGNAT
    let rank = |r: &str| {
        crate::exporters::ROLES
            .iter()
            .position(|x| *x == r)
            .unwrap_or(usize::MAX)
    };
    seen_on.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then(rank(&a.role).cmp(&rank(&b.role)))
    });

    let exporter = safe_ip(params.exporter_ip.as_deref());
    let role = match (exporter, params.role.as_deref()) {
        (Some(e), _) => role_of(e),
        (None, Some(r)) if crate::exporters::valid_role(r) => r.to_string(),
        _ => seen_on
            .first()
            .map(|v| v.role.clone())
            .unwrap_or_else(|| crate::exporters::DEFAULT_ROLE.to_string()),
    };
    let device = device_cond(&state.db, exporter, Some(&role)).await;
    let classifier = crate::netclass::NetClassifier::load(&state.db).await;
    let peer_ip = format!("if({is_up}, if(nat_dst_ip != {zero}, nat_dst_ip, dst_ip), src_ip)");
    let peer_asn = format!("if({is_up}, dst_asn, src_asn)");
    let translated = format!(
        "multiIf(src_ip = {x}, nat_src_ip, nat_src_ip = {x}, src_ip, \
                 dst_ip = {x}, nat_dst_ip, nat_dst_ip = {x}, dst_ip, {zero})"
    );
    let peer_internal = classifier.sql_internal(&peer_ip, &peer_asn, ip.is_ipv6());
    let (scope, scope_cond) = match params.scope.as_deref() {
        Some("internet") => ("internet", format!(" AND NOT {peer_internal}")),
        Some("internal") => ("internal", format!(" AND {peer_internal}")),
        _ => ("all", String::new()),
    };
    let filter = format!(
        "FROM {table} WHERE {device} AND timestamp >= now() - INTERVAL {minutes} MINUTE \
         AND ({is_up} OR {is_down}){scope_cond}"
    );
    let up = format!("sumIf(bytes, {is_up}) AS up_bytes");
    let down = format!("sumIf(bytes, {is_down}) AS down_bytes");
    // Lower port is taken as the service; the ephemeral side is dropped
    let port = "least(src_port, dst_port) AS port";

    let series_sql = format!(
        "SELECT toUnixTimestamp(toStartOfInterval(timestamp, INTERVAL {bucket_secs} SECOND)) AS t, \
                {up}, {down}, anyIf(src_asn, src_ip = {x}) AS self_asn \
         {filter} GROUP BY t ORDER BY t FORMAT JSON"
    );
    let conv_sql = format!(
        "SELECT toString({peer_ip}) AS peer, \
                {peer_asn} AS peer_asn, protocol, {port}, {up}, {down}, \
                if({translated} = {zero}, '', toString({translated})) AS translated, \
                sum(packets) AS packets, \
                toUnixTimestamp(min(timestamp)) AS first_seen, \
                toUnixTimestamp(max(timestamp)) AS last_seen \
         {filter} GROUP BY peer, peer_asn, protocol, port, translated \
         ORDER BY up_bytes + down_bytes DESC LIMIT 50 FORMAT JSON"
    );
    let ports_sql = format!(
        "SELECT protocol, {port}, {up}, {down}, \
                uniq({peer_ip}) AS peers \
         {filter} GROUP BY protocol, port \
         ORDER BY up_bytes + down_bytes DESC LIMIT 20 FORMAT JSON"
    );

    let (series_val, conv_val, ports_val) = tokio::try_join!(
        ch_query(&series_sql),
        ch_query(&conv_sql),
        ch_query(&ports_sql)
    )?;
    let data = |v: &serde_json::Value| v["data"].as_array().cloned().unwrap_or_default();

    let (mut up_bytes, mut down_bytes, mut self_asn) = (0u64, 0u64, 0u64);
    let series: Vec<TalkerPoint> = data(&series_val)
        .iter()
        .map(|r| {
            let up = parse_u64_field(&r["up_bytes"]);
            let down = parse_u64_field(&r["down_bytes"]);
            up_bytes += up;
            down_bytes += down;
            self_asn = self_asn.max(parse_u64_field(&r["self_asn"]));
            TalkerPoint {
                t: parse_u64_field(&r["t"]),
                up_bps: up * 8 / bucket_secs,
                down_bps: down * 8 / bucket_secs,
            }
        })
        .collect();

    let buckets = (minutes as u64 * 60 / bucket_secs) as u32;
    let up_rates: Vec<u64> = series.iter().map(|p| p.up_bps).collect();
    let down_rates: Vec<u64> = series.iter().map(|p| p.down_bps).collect();

    let conversations = data(&conv_val)
        .iter()
        .map(|r| TalkerConversation {
            peer: r["peer"].as_str().unwrap_or_default().to_string(),
            peer_asn: parse_u64_field(&r["peer_asn"]),
            peer_class: classifier.classify_str(
                r["peer"].as_str().unwrap_or_default(),
                parse_u64_field(&r["peer_asn"]) as u32,
            ),
            translated: r["translated"].as_str().unwrap_or_default().to_string(),
            protocol: parse_u64_field(&r["protocol"]),
            port: parse_u64_field(&r["port"]),
            up_bytes: parse_u64_field(&r["up_bytes"]),
            down_bytes: parse_u64_field(&r["down_bytes"]),
            packets: parse_u64_field(&r["packets"]),
            first_seen: parse_u64_field(&r["first_seen"]),
            last_seen: parse_u64_field(&r["last_seen"]),
        })
        .collect();

    let ports = data(&ports_val)
        .iter()
        .map(|r| TalkerPort {
            protocol: parse_u64_field(&r["protocol"]),
            port: parse_u64_field(&r["port"]),
            up_bytes: parse_u64_field(&r["up_bytes"]),
            down_bytes: parse_u64_field(&r["down_bytes"]),
            peers: parse_u64_field(&r["peers"]),
        })
        .collect();

    let to = unix_now();
    Ok(Json(TalkerDetail {
        ip: ip.to_string(),
        ip_class: classifier.classify(ip, self_asn as u32),
        scope,
        role,
        seen_on,
        bucket_secs,
        from: to - minutes as u64 * 60,
        to,
        up_bytes,
        down_bytes,
        up_p95_bps: p95_zero_filled(up_rates, buckets),
        down_p95_bps: p95_zero_filled(down_rates, buckets),
        series,
        conversations,
        ports,
    }))
}

/// Nearest-rank p95 over `buckets` slots, where `values` are the non-empty ones
fn p95_zero_filled(mut values: Vec<u64>, buckets: u32) -> u64 {
    values.sort_unstable_by(|a, b| b.cmp(a));
    let pos = p95_desc_position(buckets.max(values.len() as u32));
    values.get(pos as usize - 1).copied().unwrap_or(0)
}

#[cfg(test)]
mod talker_tests {
    use super::p95_zero_filled;

    #[test]
    fn p95_counts_silent_buckets_as_zero() {
        // one burst in 60 buckets: rank 57 ascending is a zero
        assert_eq!(p95_zero_filled(vec![1_000], 60), 0);
        // traffic in every bucket: 4th largest
        let full: Vec<u64> = (1..=60).collect();
        assert_eq!(p95_zero_filled(full, 60), 57);
        assert_eq!(p95_zero_filled(vec![], 300), 0);
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Only a syntactically valid IP may be interpolated into SQL
fn safe_ip(ip: Option<&str>) -> Option<&str> {
    ip.filter(|s| s.parse::<std::net::IpAddr>().is_ok())
}

/// Last `minutes` complete minutes (task 17.2 R-03): the open minute would
/// drag rates down
fn complete_minutes_where(device: &str, minutes: u32) -> String {
    format!(
        "WHERE {device} AND timestamp >= toStartOfMinute(now()) - INTERVAL {minutes} MINUTE \
         AND timestamp < toStartOfMinute(now())"
    )
}

/// Device scope as a SQL condition (task 17.9 R-02): one exporter, or every
/// enabled exporter of a role; border by default. Never sums across roles.
async fn device_cond(
    db: &sqlx::SqlitePool,
    exporter_ip: Option<&str>,
    role: Option<&str>,
) -> String {
    if let Some(ip) = safe_ip(exporter_ip) {
        return format!("exporter_ip = '{ip}'");
    }
    let role = role
        .filter(|r| crate::exporters::valid_role(r))
        .unwrap_or(crate::exporters::DEFAULT_ROLE);
    exporters_in(&crate::exporters::ips_for_role(db, role).await)
}

fn exporters_in(ips: &[String]) -> String {
    let list: Vec<String> = ips
        .iter()
        .filter_map(|ip| ip.parse::<std::net::IpAddr>().ok())
        .map(|ip| format!("'{ip}'"))
        .collect();
    if list.is_empty() {
        "0".to_string()
    } else {
        format!("exporter_ip IN ({})", list.join(", "))
    }
}

/// Nearest-rank p95 (bps) over `minutes` zero-filled buckets, given a column
/// holding per-minute byte sums inside the aggregation
fn p95_bps_expr(col: &str, minutes: u32) -> String {
    let pos = p95_desc_position(minutes);
    format!("intDiv(arrayElement(arrayReverseSort(groupArray({col})), {pos}) * 8, 60)")
}

fn avg_bps(bytes: u64, minutes: u32) -> u64 {
    bytes * 8 / (minutes as u64 * 60)
}

fn where_clause(device: &str, window: u32, unit: &str) -> String {
    format!("WHERE {device} AND timestamp >= now() - INTERVAL {window} {unit}")
}

fn parse_u64_field(v: &serde_json::Value) -> u64 {
    match v {
        serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
        serde_json::Value::String(s) => s.parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

/// Parses the `data` array from a FORMAT JSON response into a Vec<T>.
/// Rows that fail to deserialize are silently skipped.
fn parse_rows<T>(val: &serde_json::Value) -> Vec<TopTalkerRow>
where
    T: for<'de> Deserialize<'de> + Into<TopTalkerRow>,
{
    val["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| serde_json::from_value::<T>(r).ok())
        .map(Into::into)
        .collect()
}

/// Serde helper: ClickHouse sometimes returns numbers as quoted strings
#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrU64 {
    Num(u64),
    Str(String),
}

impl From<StringOrU64> for u64 {
    fn from(v: StringOrU64) -> u64 {
        match v {
            StringOrU64::Num(n) => n,
            StringOrU64::Str(s) => s.parse().unwrap_or(0),
        }
    }
}
