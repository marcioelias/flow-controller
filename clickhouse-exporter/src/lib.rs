use clickhouse::{Client, Row};
use serde::Serialize;
use anyhow::Result;

/// A row representing an aggregated IPv4 flow
#[derive(Row, Serialize, Clone, Debug)]
pub struct NetworkFlowV4Row {
    pub timestamp: u32,
    pub exporter_ip: String,
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,
    pub dst_asn: u32,
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
}

/// A row representing an aggregated IPv6 flow
#[derive(Row, Serialize, Clone, Debug)]
pub struct NetworkFlowV6Row {
    pub timestamp: u32,
    pub exporter_ip: String,
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,
    pub dst_asn: u32,
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
}

pub struct ClickhouseExporter {
    client: Client,
}

impl ClickhouseExporter {
    pub fn new(url: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_database("default");
        
        Self { client }
    }

    /// Creates the necessary ClickHouse tables automatically if they don't exist
    pub async fn setup_tables(&self) -> Result<()> {
        let retention_days: u32 = std::env::var("FLOW_RETENTION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let ddl_v4 = format!(r#"
            CREATE TABLE IF NOT EXISTS network_flows_v4
            (
                timestamp DateTime,
                exporter_ip String,
                src_ip String,
                dst_ip String,
                src_port UInt16,
                dst_port UInt16,
                protocol UInt8,
                src_asn UInt32,
                dst_asn UInt32,
                packets UInt64,
                bytes UInt64,
                flow_count UInt64
            )
            ENGINE = MergeTree()
            PARTITION BY toYYYYMMDD(timestamp)
            ORDER BY (timestamp, exporter_ip, src_ip, dst_ip, protocol)
            TTL timestamp + INTERVAL {retention_days} DAY
            SETTINGS index_granularity = 8192
        "#);

        let ddl_v6 = format!(r#"
            CREATE TABLE IF NOT EXISTS network_flows_v6
            (
                timestamp DateTime,
                exporter_ip String,
                src_ip String,
                dst_ip String,
                src_port UInt16,
                dst_port UInt16,
                protocol UInt8,
                src_asn UInt32,
                dst_asn UInt32,
                packets UInt64,
                bytes UInt64,
                flow_count UInt64
            )
            ENGINE = MergeTree()
            PARTITION BY toYYYYMMDD(timestamp)
            ORDER BY (timestamp, exporter_ip, src_ip, dst_ip, protocol)
            TTL timestamp + INTERVAL {retention_days} DAY
            SETTINGS index_granularity = 8192
        "#);

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
        ];
        for sql in &alters {
            self.client.query(*sql).execute().await?;
        }

        // Apply or update TTL for existing installations
        let ttl_alters = [
            format!("ALTER TABLE network_flows_v4 MODIFY TTL timestamp + INTERVAL {retention_days} DAY"),
            format!("ALTER TABLE network_flows_v6 MODIFY TTL timestamp + INTERVAL {retention_days} DAY"),
        ];
        for sql in &ttl_alters {
            if let Err(e) = self.client.query(sql.as_str()).execute().await {
                tracing::warn!("TTL alter warning (non-fatal): {e}");
            }
        }

        // Anomaly detector materialized views (hourly tx/rx per IP)
        let mv_tx = format!(r#"
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
        "#);

        let mv_rx = format!(r#"
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
        "#);

        if let Err(e) = self.client.query(mv_tx.as_str()).execute().await {
            tracing::warn!("ip_hourly_tx_v4 MV create (non-fatal): {e}");
        }
        if let Err(e) = self.client.query(mv_rx.as_str()).execute().await {
            tracing::warn!("ip_hourly_rx_v4 MV create (non-fatal): {e}");
        }

        Ok(())
    }

    /// Inserts a batch of IPv4 flows and IPv6 flows
    pub async fn insert_batch(&self, v4_batch: &[NetworkFlowV4Row], v6_batch: &[NetworkFlowV6Row]) -> Result<()> {
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
}
