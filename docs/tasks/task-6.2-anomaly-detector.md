# Task 6.2 — Anomaly Detector Engine

## Goal

Implement the core detection logic as a background Tokio task.
The detector runs every `eval_interval` (default 60 s), loads all enabled rules from
SQLite, evaluates each against ClickHouse, and writes `alert_events` for anomalies.
It does not send Telegram notifications — that is task 6.3.

---

## What constitutes an anomaly

Two rule types, grounded in observable network behavior rather than volume alone:

### 1. `upload_inversion`

A residential/PPPoE subscriber is asymmetric by nature: they download far more than they
upload (streaming, browsing, gaming). When a subscriber starts uploading more than it
downloads — sustained over a meaningful window — the host is likely:
- Part of a botnet (C2 beaconing, DDoS participation)
- Running an undeclared server (spam relay, crypto miner pool member)
- Experiencing a routing loop

Detection logic: compare the ratio `upload / download` in the current short window against
the historical ratio for that IP. Alert only when all conditions hold simultaneously:
- Current upload > current download × `inversion_ratio`
- Historical pattern confirms this IP was download-heavy (rules out servers and P2P nodes
  that are legitimately symmetric — they'd never pass `min_hist_download_ratio`)
- Upload exceeds `min_upload_mbps` (filters noise from near-idle IPs)

### 2. `attack_signature`

High PPS + small average packet size + destination to known attack ports is a
near-zero-false-positive signal that a subscriber is actively sending attack traffic:
- SYN flood: 40–60 byte packets, high PPS, ports 80/443
- UDP amplification: small request packets to NTP/SSDP/Memcached/DNS
- Brute force: 40–80 byte TCP packets, ports 22/3389/25

No history required. The signal is self-evident in a 2-minute window.

---

## Query cost analysis & optimization

The `upload_inversion` rule requires a 24h historical baseline per IP.
Querying `network_flows_v4` directly every 60s for 24h would scan hundreds of millions of
rows — unacceptable.

**Solution: ClickHouse Materialized Views with `SummingMergeTree`**

Two summary views are created at startup (in `clickhouse-exporter/src/lib.rs`,
alongside `setup_tables()`). They aggregate flows into hourly buckets per IP in
real time at insert cost, not query cost:

```sql
-- Upload view: tracks bytes/packets sent BY each subscriber (src_ip perspective)
CREATE MATERIALIZED VIEW IF NOT EXISTS ip_hourly_tx_v4
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMMDD(hour)
ORDER BY (hour, exporter_ip, src_ip)
POPULATE AS
SELECT
    toStartOfHour(timestamp)  AS hour,
    exporter_ip,
    src_ip,
    sum(bytes)                AS bytes,
    sum(packets)              AS packets
FROM network_flows_v4
GROUP BY hour, exporter_ip, src_ip;

-- Download view: tracks bytes/packets received BY each subscriber (dst_ip perspective)
CREATE MATERIALIZED VIEW IF NOT EXISTS ip_hourly_rx_v4
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMMDD(hour)
ORDER BY (hour, exporter_ip, dst_ip)
POPULATE AS
SELECT
    toStartOfHour(timestamp)  AS hour,
    exporter_ip,
    dst_ip                    AS src_ip,   -- alias for uniform join
    sum(bytes)                AS bytes,
    sum(packets)              AS packets
FROM network_flows_v4
GROUP BY hour, exporter_ip, dst_ip;
```

### Disk cost (MVs are cheap)

Each MV row is ~56 bytes before LZ4 compression (~8–10 bytes after).

| Setup | MV rows/day | Compressed size/day |
|-------|-------------|---------------------|
| 1000 subscribers | 48k | ~0.5 MB |
| 25k subscribers | 1.2M | ~12 MB |
| 25k × 30-day retention | 36M | ~360 MB |

Compare to `network_flows_v4` raw: ~34 GB/day at 5k flows/sec.
**The MVs add < 0.1% of raw table disk usage.**

### TTL on MVs (required — do not skip)

Without a TTL the MV grows forever independently of the raw table's TTL.
Add TTL to both MV DDL statements:

```sql
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMMDD(hour)
ORDER BY (hour, exporter_ip, src_ip)
TTL hour + INTERVAL 30 DAY   -- must match FLOW_RETENTION_DAYS
```

The TTL value is read from the same `FLOW_RETENTION_DAYS` env var used by the raw tables.
Interpolate it at startup just like the existing TTL in `setup_tables()`.

### Query cost comparison (25k subscribers, 5k flows/sec)

| Window | Table | Rows scanned | Latency (est.) |
|--------|-------|-------------|----------------|
| 10 min short (raw) | `network_flows_v4` | ~3M | ~20ms |
| 24h history (MV) | `ip_hourly_tx/rx_v4` | ~600k (JOIN) | ~10ms |
| Per eval cycle (60s) | both | ~3.6M | ~30ms total |

The short window still queries the raw table for recency. The long window queries the MV.
Net result: each 60s eval cycle completes in <50ms even on a loaded BNG.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/detector.rs` | CREATE |
| `collector-core/src/main.rs` | MODIFY — spawn detector task |
| `clickhouse-exporter/src/lib.rs` | MODIFY — create MV tables in `setup_tables()` |

---

## `clickhouse-exporter/src/lib.rs` additions

Add to `setup_tables()`, after the existing CREATE TABLE statements:

```rust
// Upload MV (tx)
client.execute(r#"
    CREATE MATERIALIZED VIEW IF NOT EXISTS ip_hourly_tx_v4
    ENGINE = SummingMergeTree()
    PARTITION BY toYYYYMMDD(hour)
    ORDER BY (hour, exporter_ip, src_ip)
    POPULATE AS
    SELECT toStartOfHour(timestamp) AS hour, exporter_ip, src_ip,
           sum(bytes) AS bytes, sum(packets) AS packets
    FROM network_flows_v4
    GROUP BY hour, exporter_ip, src_ip
"#).await?;

// Download MV (rx)
client.execute(r#"
    CREATE MATERIALIZED VIEW IF NOT EXISTS ip_hourly_rx_v4
    ENGINE = SummingMergeTree()
    PARTITION BY toYYYYMMDD(hour)
    ORDER BY (hour, exporter_ip, src_ip)
    POPULATE AS
    SELECT toStartOfHour(timestamp) AS hour, exporter_ip,
           dst_ip AS src_ip,
           sum(bytes) AS bytes, sum(packets) AS packets
    FROM network_flows_v4
    GROUP BY hour, exporter_ip, dst_ip
"#).await?;
```

---

## `detector.rs` structure

```rust
use std::sync::Arc;
use crate::auth::AppState;
use crate::alerts::{self, AlertEvent, AlertRule, AlertSeverity, DEFAULT_ATTACK_PORTS};

const EVAL_INTERVAL_SECS: u64 = 60;

pub async fn run_detector(state: Arc<AppState>) {
    tracing::info!("Alert detector started");
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(EVAL_INTERVAL_SECS)).await;
        let rules = match load_enabled_rules(&state.db).await {
            Ok(r) => r,
            Err(e) => { tracing::error!("load_enabled_rules: {e}"); continue; }
        };
        for rule in rules {
            if let Err(e) = evaluate_rule(&state, &rule).await {
                tracing::warn!("Rule '{}' eval error: {e}", rule.name);
            }
        }
    }
}
```

---

## `eval_upload_inversion`

### ClickHouse queries

**Short window — current upload/download (raw table, fast)**
```sql
-- Upload: bytes sent by each src_ip in last {short_min} minutes
SELECT src_ip, sum(bytes) AS upload_bytes, sum(packets) AS upload_packets
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {short_min} MINUTE
  AND exporter_ip = '{exporter_ip}'
GROUP BY src_ip
HAVING upload_bytes >= {min_upload_bytes}
FORMAT JSON
```

```sql
-- Download: bytes received by each src_ip in last {short_min} minutes
SELECT dst_ip AS src_ip, sum(bytes) AS download_bytes
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {short_min} MINUTE
  AND exporter_ip = '{exporter_ip}'
  AND dst_ip IN ({ip_list_from_upload_query})
GROUP BY dst_ip
FORMAT JSON
```

**History window — baseline ratio (MV, cheap)**
```sql
-- Historical upload/download ratio per IP over last {history_h} hours
SELECT
    tx.src_ip,
    sum(tx.bytes)  AS hist_upload,
    sum(rx.bytes)  AS hist_download,
    sum(rx.bytes) / greatest(sum(tx.bytes), 1) AS hist_dl_ul_ratio
FROM ip_hourly_tx_v4 tx
JOIN ip_hourly_rx_v4 rx ON rx.src_ip = tx.src_ip
                        AND rx.hour   = tx.hour
                        AND rx.exporter_ip = tx.exporter_ip
WHERE tx.hour >= toStartOfHour(now()) - INTERVAL {history_h} HOUR
  AND tx.exporter_ip = '{exporter_ip}'
  AND tx.src_ip IN ({ip_list})
GROUP BY tx.src_ip
HAVING hist_dl_ul_ratio >= {min_hist_download_ratio}
FORMAT JSON
```

### Decision logic (in Rust, after queries return)

```rust
// For each IP that passes both queries:
//   current_ul > current_dl * inversion_ratio  → warning
//   current_ul > current_dl * inversion_ratio * 2.0  → critical
// Also check cooldown before inserting event.
```

---

## `eval_attack_signature`

### ClickHouse query (raw table, 2-minute window — always fast)

```sql
SELECT
    src_ip,
    sum(packets)                                  AS total_packets,
    sum(packets) / ({window_min} * 60.0)          AS pps,
    sum(bytes) / greatest(sum(packets), 1)        AS avg_pkt_bytes,
    groupArray(dst_port)                          AS ports_seen
FROM network_flows_v4
WHERE timestamp >= now() - INTERVAL {window_min} MINUTE
  AND exporter_ip = '{exporter_ip}'
GROUP BY src_ip
HAVING total_packets >= {min_total_packets}
   AND pps            >= {min_pps}
   AND avg_pkt_bytes  <= {max_avg_pkt_bytes}
FORMAT JSON
```

After query returns, filter in Rust: keep only rows where at least one port in
`ports_seen` is in the rule's `attack_ports` list (or `DEFAULT_ATTACK_PORTS` if empty).

### Decision logic

```rust
// All three conditions must hold (enforced by HAVING in query + port filter in Rust):
//   pps >= min_pps               → high packet rate
//   avg_pkt_bytes <= max_avg     → small packets
//   dst_port in attack_ports     → known attack destination
// Severity:
//   pps >= min_pps * 5  → critical
//   otherwise           → warning
```

---

## ClickHouse query helper

Use `reqwest` to POST SQL to `{clickhouse_url}?query=...&default_format=JSON`.
Parse the `{"data": [...], "rows": N}` envelope. Do not use the clickhouse-rs streaming
client — ad-hoc analytics queries don't need it.

```rust
pub async fn ch_query<T: serde::de::DeserializeOwned>(
    url: &str,
    sql: &str,
) -> anyhow::Result<Vec<T>> {
    let resp = reqwest::Client::new()
        .post(url)
        .query(&[("default_format", "JSON")])
        .body(sql.to_string())
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let rows = resp["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("no data array in response"))?;

    rows.iter()
        .map(|v| serde_json::from_value(v.clone()).map_err(Into::into))
        .collect()
}
```

---

## `main.rs` changes

```rust
let detector_state = app_state.clone();
rt.spawn(async move {
    detector::run_detector(detector_state).await;
});
```

---

## Acceptance criteria

- [ ] `cargo build` passes
- [ ] Materialized views created at startup without error
- [ ] `upload_inversion`: IP that has 10:1 download/upload history and then inverts
      (uploads 5× downloads) generates a warning event
- [ ] `upload_inversion`: IP that is historically symmetric (P2P node with 1:1 ratio)
      does not generate an event (fails `min_hist_download_ratio` check)
- [ ] `attack_signature`: IP with 5000 PPS, 60-byte avg, dst_port 80 generates critical
- [ ] `attack_signature`: IP with 5000 PPS, 1400-byte avg (bulk transfer) does not fire
- [ ] `attack_signature`: IP with 5000 PPS, 60-byte avg, dst_port 9999 (not in list) does not fire
- [ ] Cooldown prevents duplicate events within the configured window
- [ ] Eval cycle logs a warning on ClickHouse query error but continues to next rule
