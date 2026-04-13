use clickhouse::{Client, Row};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};
use anyhow::Result;

/// A row representing an aggregated IPv4 flow
#[derive(Row, Serialize, Clone, Debug)]
pub struct NetworkFlowV4Row {
    pub timestamp: u32,
    pub src_ip: Ipv4Addr,
    pub dst_ip: Ipv4Addr,
    pub dst_port: u16,
    pub protocol: u8,
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
}

/// A row representing an aggregated IPv6 flow
#[derive(Row, Serialize, Clone, Debug)]
pub struct NetworkFlowV6Row {
    pub timestamp: u32,
    pub src_ip: Ipv6Addr,
    pub dst_ip: Ipv6Addr,
    pub dst_port: u16,
    pub protocol: u8,
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
        let ddl_v4 = r#"
            CREATE TABLE IF NOT EXISTS network_flows_v4
            (
                timestamp DateTime,
                src_ip IPv4,
                dst_ip IPv4,
                dst_port UInt16,
                protocol UInt8,
                packets UInt64,
                bytes UInt64,
                flow_count UInt64
            ) 
            ENGINE = MergeTree()
            PARTITION BY toYYYYMMDD(timestamp)
            ORDER BY (timestamp, src_ip, dst_ip, protocol)
        "#;

        let ddl_v6 = r#"
            CREATE TABLE IF NOT EXISTS network_flows_v6
            (
                timestamp DateTime,
                src_ip IPv6,
                dst_ip IPv6,
                dst_port UInt16,
                protocol UInt8,
                packets UInt64,
                bytes UInt64,
                flow_count UInt64
            ) 
            ENGINE = MergeTree()
            PARTITION BY toYYYYMMDD(timestamp)
            ORDER BY (timestamp, src_ip, dst_ip, protocol)
        "#;

        self.client.query(ddl_v4).execute().await?;
        self.client.query(ddl_v6).execute().await?;

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
