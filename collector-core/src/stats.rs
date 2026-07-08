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
        .post(&clickhouse_url())
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
    State(_state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<ProtocolStatsQuery>,
) -> Result<Json<ProtocolStats>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).min(1440);
    let wc = where_clause(params.exporter_ip.as_deref(), minutes, "MINUTE");

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

    Ok(Json(ProtocolStats { tcp, udp, icmp, other }))
}

// ─── 2.1 Top talkers ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TopTalkersQuery {
    pub exporter_ip: Option<String>,
    pub minutes: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopTalkerRow {
    pub src_ip: String,
    pub total_bytes: u64,
    pub total_packets: u64,
    pub flow_count: u64,
}

pub async fn get_top_talkers_handler(
    State(_state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<TopTalkersQuery>,
) -> Result<Json<Vec<TopTalkerRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).min(1440);
    let limit = params.limit.unwrap_or(20).min(100);
    let wc = where_clause(params.exporter_ip.as_deref(), minutes, "MINUTE");

    let sql = format!(
        "SELECT src_ip, sum(bytes) AS total_bytes, sum(packets) AS total_packets, \
                sum(flow_count) AS flow_count \
         FROM network_flows_v4 {wc} GROUP BY src_ip ORDER BY total_bytes DESC LIMIT {limit} \
         UNION ALL \
         SELECT src_ip, sum(bytes) AS total_bytes, sum(packets) AS total_packets, \
                sum(flow_count) AS flow_count \
         FROM network_flows_v6 {wc} GROUP BY src_ip ORDER BY total_bytes DESC LIMIT {limit} \
         FORMAT JSON"
    );

    let val = ch_query(&sql).await?;
    let mut rows = parse_rows::<TopTalkerRowRaw>(&val);
    rows.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    rows.truncate(limit as usize);
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct TopTalkerRowRaw {
    src_ip: String,
    total_bytes: StringOrU64,
    total_packets: StringOrU64,
    flow_count: StringOrU64,
}

impl From<TopTalkerRowRaw> for TopTalkerRow {
    fn from(r: TopTalkerRowRaw) -> Self {
        TopTalkerRow {
            src_ip: r.src_ip,
            total_bytes: r.total_bytes.into(),
            total_packets: r.total_packets.into(),
            flow_count: r.flow_count.into(),
        }
    }
}

// ─── 2.2 ASN traffic ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AsnQuery {
    pub exporter_ip: Option<String>,
    pub minutes: Option<u32>,
    pub limit: Option<u32>,
    pub direction: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AsnRow {
    pub asn: u64,
    pub label: String,
    pub total_bytes: u64,
    pub total_packets: u64,
}

pub async fn get_asn_stats_handler(
    State(_state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<AsnQuery>,
) -> Result<Json<Vec<AsnRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(60).min(1440);
    let limit = params.limit.unwrap_or(20).min(100);
    let direction = params.direction.as_deref().unwrap_or("both");

    let mut merged: HashMap<u64, (u64, u64)> = HashMap::new();

    for table in &["network_flows_v4", "network_flows_v6"] {
        let sql = build_asn_query(table, params.exporter_ip.as_deref(), minutes, limit, direction);
        let val = ch_query(&sql).await?;
        for row in val["data"].as_array().cloned().unwrap_or_default() {
            let asn = parse_u64_field(&row["asn"]);
            let bytes = parse_u64_field(&row["total_bytes"]);
            let pkts = parse_u64_field(&row["total_packets"]);
            let e = merged.entry(asn).or_insert((0, 0));
            e.0 += bytes;
            e.1 += pkts;
        }
    }

    let mut rows: Vec<AsnRow> = merged
        .into_iter()
        .map(|(asn, (bytes, pkts))| AsnRow {
            asn,
            label: if asn == 0 { "Unknown".to_string() } else { format!("AS{asn}") },
            total_bytes: bytes,
            total_packets: pkts,
        })
        .collect();

    rows.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    rows.truncate(limit as usize);
    Ok(Json(rows))
}

fn build_asn_query(table: &str, exporter_ip: Option<&str>, minutes: u32, limit: u32, direction: &str) -> String {
    let time_filter = match exporter_ip {
        Some(ip) => format!(
            "WHERE exporter_ip = '{}' AND timestamp >= now() - INTERVAL {} MINUTE",
            ip, minutes
        ),
        None => format!("WHERE timestamp >= now() - INTERVAL {} MINUTE", minutes),
    };

    match direction {
        "src" => format!(
            "SELECT src_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets \
             FROM {table} {time_filter} AND src_asn != 0 \
             GROUP BY src_asn ORDER BY total_bytes DESC LIMIT {limit} FORMAT JSON"
        ),
        "dst" => format!(
            "SELECT dst_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets \
             FROM {table} {time_filter} AND dst_asn != 0 \
             GROUP BY dst_asn ORDER BY total_bytes DESC LIMIT {limit} FORMAT JSON"
        ),
        _ => format!(
            "SELECT asn, sum(total_bytes) AS total_bytes, sum(total_packets) AS total_packets FROM (\
               SELECT src_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets \
               FROM {table} {time_filter} AND src_asn != 0 GROUP BY src_asn \
               UNION ALL \
               SELECT dst_asn AS asn, sum(bytes) AS total_bytes, sum(packets) AS total_packets \
               FROM {table} {time_filter} AND dst_asn != 0 GROUP BY dst_asn\
             ) GROUP BY asn ORDER BY total_bytes DESC LIMIT {limit} FORMAT JSON"
        ),
    }
}

// ─── 2.3 Port breakdown ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PortBreakdownQuery {
    pub exporter_ip: Option<String>,
    pub minutes: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PortRow {
    pub dst_port: u16,
    pub service: String,
    pub total_bytes: u64,
    pub total_packets: u64,
}

fn port_to_service(port: u16) -> &'static str {
    match port {
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
    State(_state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<PortBreakdownQuery>,
) -> Result<Json<Vec<PortRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).min(1440);
    let limit = params.limit.unwrap_or(20).min(100);
    let wc = where_clause(params.exporter_ip.as_deref(), minutes, "MINUTE");

    let sql = format!(
        "SELECT dst_port, sum(bytes) AS total_bytes, sum(packets) AS total_packets \
         FROM network_flows_v4 {wc} GROUP BY dst_port ORDER BY total_bytes DESC LIMIT {limit} \
         UNION ALL \
         SELECT dst_port, sum(bytes) AS total_bytes, sum(packets) AS total_packets \
         FROM network_flows_v6 {wc} GROUP BY dst_port ORDER BY total_bytes DESC LIMIT {limit} \
         FORMAT JSON"
    );

    let val = ch_query(&sql).await?;
    let mut merged: HashMap<u16, (u64, u64)> = HashMap::new();

    for row in val["data"].as_array().cloned().unwrap_or_default() {
        let port = parse_u64_field(&row["dst_port"]) as u16;
        let bytes = parse_u64_field(&row["total_bytes"]);
        let pkts = parse_u64_field(&row["total_packets"]);
        let e = merged.entry(port).or_insert((0, 0));
        e.0 += bytes;
        e.1 += pkts;
    }

    let mut rows: Vec<PortRow> = merged
        .into_iter()
        .map(|(port, (bytes, pkts))| PortRow {
            dst_port: port,
            service: port_to_service(port).to_string(),
            total_bytes: bytes,
            total_packets: pkts,
        })
        .collect();

    rows.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    rows.truncate(limit as usize);
    Ok(Json(rows))
}

// ─── 2.4 Traffic timeline ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub exporter_ip: Option<String>,
    pub hours: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub minute: u64,
    pub total_bytes: u64,
    pub total_packets: u64,
}

pub async fn get_timeline_handler(
    State(_state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<TimelineQuery>,
) -> Result<Json<Vec<TimelinePoint>>, StatusCode> {
    let hours = params.hours.unwrap_or(1).min(24);
    let wc = where_clause(params.exporter_ip.as_deref(), hours, "HOUR");

    let sql = format!(
        "SELECT toUnixTimestamp(toStartOfMinute(timestamp)) AS minute, \
                sum(bytes) AS total_bytes, sum(packets) AS total_packets \
         FROM network_flows_v4 {wc} GROUP BY minute ORDER BY minute ASC \
         UNION ALL \
         SELECT toUnixTimestamp(toStartOfMinute(timestamp)) AS minute, \
                sum(bytes) AS total_bytes, sum(packets) AS total_packets \
         FROM network_flows_v6 {wc} GROUP BY minute ORDER BY minute ASC \
         FORMAT JSON"
    );

    let val = ch_query(&sql).await?;
    let mut map: BTreeMap<u64, (u64, u64)> = BTreeMap::new();

    for row in val["data"].as_array().cloned().unwrap_or_default() {
        let minute = parse_u64_field(&row["minute"]);
        let bytes = parse_u64_field(&row["total_bytes"]);
        let pkts = parse_u64_field(&row["total_packets"]);
        let e = map.entry(minute).or_insert((0, 0));
        e.0 += bytes;
        e.1 += pkts;
    }

    let points = map
        .into_iter()
        .map(|(minute, (bytes, pkts))| TimelinePoint {
            minute,
            total_bytes: bytes,
            total_packets: pkts,
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
}

pub async fn get_exporter_summary_handler(
    State(_state): State<Arc<crate::auth::AppState>>,
    Query(params): Query<ExporterSummaryQuery>,
) -> Result<Json<Vec<ExporterSummaryRow>>, StatusCode> {
    let minutes = params.minutes.unwrap_or(5).min(1440);
    let time_filter = format!("WHERE timestamp >= now() - INTERVAL {} MINUTE", minutes);

    let sql = format!(
        "SELECT exporter_ip, sum(bytes) AS total_bytes, sum(flow_count) AS flow_count, \
                uniq(src_ip) AS unique_sources \
         FROM network_flows_v4 {time_filter} GROUP BY exporter_ip ORDER BY total_bytes DESC \
         UNION ALL \
         SELECT exporter_ip, sum(bytes) AS total_bytes, sum(flow_count) AS flow_count, \
                uniq(src_ip) AS unique_sources \
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

        let e = merged.entry(ip.clone()).or_insert(ExporterSummaryRow {
            exporter_ip: ip,
            total_bytes: 0,
            flow_count: 0,
            unique_sources: 0,
        });
        e.total_bytes += bytes;
        e.flow_count += flows;
        e.unique_sources += uniq;
    }

    let mut rows: Vec<ExporterSummaryRow> = merged.into_values().collect();
    rows.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    Ok(Json(rows))
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

fn where_clause(exporter_ip: Option<&str>, window: u32, unit: &str) -> String {
    match exporter_ip {
        Some(ip) => format!(
            "WHERE exporter_ip = '{}' AND timestamp >= now() - INTERVAL {} {}",
            ip, window, unit
        ),
        None => format!("WHERE timestamp >= now() - INTERVAL {} {}", window, unit),
    }
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
