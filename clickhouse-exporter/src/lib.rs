use anyhow::Result;
use clickhouse::{Client, Row};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Aggregated IPv4 flow. IPs are numeric — RowBinary maps `u32` straight onto
/// a ClickHouse `IPv4` column with no allocation.
#[derive(Row, Serialize, Clone, Debug)]
pub struct NetworkFlowV4Row {
    pub timestamp: u32,
    pub exporter_ip: u32,
    pub src_ip: u32,
    pub dst_ip: u32,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,
    pub dst_asn: u32,
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
    /// 0 = ingress, 1 = egress, 255 = not reported (IE 61)
    pub direction: u8,
}

/// Aggregated IPv6 flow. `[u8; 16]` maps onto a ClickHouse `IPv6` column.
#[derive(Row, Serialize, Clone, Debug)]
pub struct NetworkFlowV6Row {
    pub timestamp: u32,
    pub exporter_ip: u32, // exporters are IPv4 (see exporter_v4 in the parser)
    pub src_ip: [u8; 16],
    pub dst_ip: [u8; 16],
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,
    pub dst_asn: u32,
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
    /// 0 = ingress, 1 = egress, 255 = not reported (IE 61)
    pub direction: u8,
}

/// String-typed rows for installations still on the legacy `String` schema
#[derive(Row, Serialize, Clone, Debug)]
struct LegacyRow {
    timestamp: u32,
    exporter_ip: String,
    src_ip: String,
    dst_ip: String,
    src_port: u16,
    dst_port: u16,
    protocol: u8,
    src_asn: u32,
    dst_asn: u32,
    packets: u64,
    bytes: u64,
    flow_count: u64,
    direction: u8,
}

pub struct ClickhouseExporter {
    client: Client,
    /// True while the connected database still uses `String` IP columns.
    /// Detected in `setup_tables`; never auto-migrated (see task 10.12).
    legacy_schema: AtomicBool,
}

impl ClickhouseExporter {
    pub fn new(url: &str) -> Self {
        let client = Client::default().with_url(url).with_database("default");

        Self {
            client,
            legacy_schema: AtomicBool::new(false),
        }
    }

    pub fn is_legacy_schema(&self) -> bool {
        self.legacy_schema.load(Ordering::Relaxed)
    }

    /// Returns the type of `src_ip` in an existing table, if the table exists
    async fn detect_ip_column_type(&self, table: &str) -> Option<String> {
        let sql = format!(
            "SELECT type FROM system.columns \
             WHERE database = currentDatabase() AND table = '{table}' AND name = 'src_ip'"
        );
        self.client.query(&sql).fetch_one::<String>().await.ok()
    }

    /// Creates the necessary ClickHouse tables automatically if they don't exist
    pub async fn setup_tables(&self) -> Result<()> {
        let retention_days: u32 = std::env::var("FLOW_RETENTION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        // Existing install on the legacy String schema? Never migrate it
        // silently — a MODIFY COLUMN rewrite on a large table is an operator
        // decision. scripts/migrate-ip-columns.sh does it explicitly.
        if let Some(ip_type) = self.detect_ip_column_type("network_flows_v4").await {
            if ip_type == "String" {
                self.legacy_schema.store(true, Ordering::Relaxed);
                tracing::warn!(
                    "network_flows_v4/v6 still use String IP columns. \
                     Native IPv4/IPv6 columns cut storage and speed up queries. \
                     Migrate explicitly with: scripts/migrate-ip-columns.sh"
                );
            }
        }

        // Fresh installs get native IP types and an ORDER BY that leads with
        // low-cardinality columns (better compression + usable primary index
        // for IP-filtered queries; day partitioning already bounds time scans)
        let ddl_v4 = format!(
            r#"
            CREATE TABLE IF NOT EXISTS network_flows_v4
            (
                timestamp DateTime,
                exporter_ip IPv4,
                src_ip IPv4,
                dst_ip IPv4,
                src_port UInt16,
                dst_port UInt16,
                protocol UInt8,
                src_asn UInt32,
                dst_asn UInt32,
                packets UInt64,
                bytes UInt64,
                flow_count UInt64,
                direction UInt8 DEFAULT 255
            )
            ENGINE = MergeTree()
            PARTITION BY toYYYYMMDD(timestamp)
            ORDER BY (exporter_ip, timestamp, src_ip, dst_ip, protocol)
            TTL timestamp + INTERVAL {retention_days} DAY
            SETTINGS index_granularity = 8192
        "#
        );

        let ddl_v6 = format!(
            r#"
            CREATE TABLE IF NOT EXISTS network_flows_v6
            (
                timestamp DateTime,
                exporter_ip IPv4,
                src_ip IPv6,
                dst_ip IPv6,
                src_port UInt16,
                dst_port UInt16,
                protocol UInt8,
                src_asn UInt32,
                dst_asn UInt32,
                packets UInt64,
                bytes UInt64,
                flow_count UInt64,
                direction UInt8 DEFAULT 255
            )
            ENGINE = MergeTree()
            PARTITION BY toYYYYMMDD(timestamp)
            ORDER BY (exporter_ip, timestamp, src_ip, dst_ip, protocol)
            TTL timestamp + INTERVAL {retention_days} DAY
            SETTINGS index_granularity = 8192
        "#
        );

        self.client.query(ddl_v4.as_str()).execute().await?;
        self.client.query(ddl_v6.as_str()).execute().await?;

        // Idempotent migrations for existing installations
        let alters = [
            "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS src_port UInt16 DEFAULT 0",
            "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS src_asn UInt32 DEFAULT 0",
            "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS dst_asn UInt32 DEFAULT 0",
            "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS src_port UInt16 DEFAULT 0",
            "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS src_asn UInt32 DEFAULT 0",
            "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS dst_asn UInt32 DEFAULT 0",
            "ALTER TABLE network_flows_v4 ADD COLUMN IF NOT EXISTS direction UInt8 DEFAULT 255",
            "ALTER TABLE network_flows_v6 ADD COLUMN IF NOT EXISTS direction UInt8 DEFAULT 255",
        ];
        for sql in &alters {
            self.client.query(sql).execute().await?;
        }

        // Apply or update TTL for existing installations
        let ttl_alters = [
            format!(
                "ALTER TABLE network_flows_v4 MODIFY TTL timestamp + INTERVAL {retention_days} DAY"
            ),
            format!(
                "ALTER TABLE network_flows_v6 MODIFY TTL timestamp + INTERVAL {retention_days} DAY"
            ),
        ];
        for sql in &ttl_alters {
            if let Err(e) = self.client.query(sql.as_str()).execute().await {
                tracing::warn!("TTL alter warning (non-fatal): {e}");
            }
        }

        // Anomaly detector materialized views (hourly tx/rx per IP)
        let mv_tx = format!(
            r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS ip_hourly_tx_v4
            ENGINE = SummingMergeTree()
            PARTITION BY toYYYYMMDD(hour)
            ORDER BY (hour, exporter_ip, src_ip)
            TTL hour + INTERVAL {retention_days} DAY
            POPULATE AS
            SELECT toStartOfHour(timestamp) AS hour, exporter_ip, src_ip,
                   sum(bytes) AS bytes, sum(packets) AS packets
            FROM network_flows_v4
            GROUP BY hour, exporter_ip, src_ip
        "#
        );

        let mv_rx = format!(
            r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS ip_hourly_rx_v4
            ENGINE = SummingMergeTree()
            PARTITION BY toYYYYMMDD(hour)
            ORDER BY (hour, exporter_ip, src_ip)
            TTL hour + INTERVAL {retention_days} DAY
            POPULATE AS
            SELECT toStartOfHour(timestamp) AS hour, exporter_ip,
                   dst_ip AS src_ip,
                   sum(bytes) AS bytes, sum(packets) AS packets
            FROM network_flows_v4
            GROUP BY hour, exporter_ip, dst_ip
        "#
        );

        if let Err(e) = self.client.query(mv_tx.as_str()).execute().await {
            tracing::warn!("ip_hourly_tx_v4 MV create (non-fatal): {e}");
        }
        if let Err(e) = self.client.query(mv_rx.as_str()).execute().await {
            tracing::warn!("ip_hourly_rx_v4 MV create (non-fatal): {e}");
        }

        Ok(())
    }

    /// Inserts with exponential backoff on transient failures.
    ///
    /// Retrying may duplicate rows if ClickHouse committed a batch but the
    /// response was lost; for traffic accounting that beats silent loss.
    pub async fn insert_batch_with_retry(
        &self,
        v4_batch: &[NetworkFlowV4Row],
        v6_batch: &[NetworkFlowV6Row],
        max_attempts: u32,
    ) -> Result<()> {
        let mut delay = std::time::Duration::from_millis(250);
        for attempt in 1..=max_attempts {
            match self.insert_batch(v4_batch, v6_batch).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt == max_attempts => return Err(e),
                Err(e) => {
                    tracing::warn!(
                        "ClickHouse insert attempt {attempt}/{max_attempts} failed: {e}; retrying in {delay:?}"
                    );
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(std::time::Duration::from_secs(4));
                }
            }
        }
        unreachable!("max_attempts >= 1")
    }

    /// Inserts a batch of IPv4 flows and IPv6 flows
    pub async fn insert_batch(
        &self,
        v4_batch: &[NetworkFlowV4Row],
        v6_batch: &[NetworkFlowV6Row],
    ) -> Result<()> {
        if self.is_legacy_schema() {
            return self.insert_batch_legacy(v4_batch, v6_batch).await;
        }

        if !v4_batch.is_empty() {
            let mut insert = self.client.insert("network_flows_v4")?;
            for row in v4_batch {
                insert.write(row).await?;
            }
            insert.end().await?;
        }

        if !v6_batch.is_empty() {
            let mut insert = self.client.insert("network_flows_v6")?;
            for row in v6_batch {
                insert.write(row).await?;
            }
            insert.end().await?;
        }

        Ok(())
    }

    /// Insert path for the legacy String schema: converts IPs to strings.
    /// The allocation cost only exists until the operator runs the migration.
    async fn insert_batch_legacy(
        &self,
        v4_batch: &[NetworkFlowV4Row],
        v6_batch: &[NetworkFlowV6Row],
    ) -> Result<()> {
        if !v4_batch.is_empty() {
            let mut insert = self.client.insert("network_flows_v4")?;
            for row in v4_batch {
                insert
                    .write(&LegacyRow {
                        timestamp: row.timestamp,
                        exporter_ip: std::net::Ipv4Addr::from(row.exporter_ip).to_string(),
                        src_ip: std::net::Ipv4Addr::from(row.src_ip).to_string(),
                        dst_ip: std::net::Ipv4Addr::from(row.dst_ip).to_string(),
                        src_port: row.src_port,
                        dst_port: row.dst_port,
                        protocol: row.protocol,
                        src_asn: row.src_asn,
                        dst_asn: row.dst_asn,
                        packets: row.packets,
                        bytes: row.bytes,
                        flow_count: row.flow_count,
                        direction: row.direction,
                    })
                    .await?;
            }
            insert.end().await?;
        }

        if !v6_batch.is_empty() {
            let mut insert = self.client.insert("network_flows_v6")?;
            for row in v6_batch {
                insert
                    .write(&LegacyRow {
                        timestamp: row.timestamp,
                        exporter_ip: std::net::Ipv4Addr::from(row.exporter_ip).to_string(),
                        src_ip: std::net::Ipv6Addr::from(row.src_ip).to_string(),
                        dst_ip: std::net::Ipv6Addr::from(row.dst_ip).to_string(),
                        src_port: row.src_port,
                        dst_port: row.dst_port,
                        protocol: row.protocol,
                        src_asn: row.src_asn,
                        dst_asn: row.dst_asn,
                        packets: row.packets,
                        bytes: row.bytes,
                        flow_count: row.flow_count,
                        direction: row.direction,
                    })
                    .await?;
            }
            insert.end().await?;
        }

        Ok(())
    }
}
