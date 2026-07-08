use std::net::{Ipv4Addr, Ipv6Addr};

/// Feature vector extracted from a 1-second aggregation window for a single (exporter, src_ip) pair.
/// Used as input to the ML anomaly detector (Isolation Forest).
#[derive(Debug, Clone)]
pub struct FlowFeatures {
    pub exporter_ip:      String,
    pub src_ip:           String,
    pub window_ts:        u32,

    pub bytes_total:      u64,
    pub packets_total:    u64,
    pub flows_total:      u64,
    pub avg_pkt_bytes:    f64,

    pub pps:              f64,
    pub bps:              f64,

    pub unique_dst_ips:   u32,
    pub unique_dst_ports: u32,

    pub upload_bytes:     u64,
    pub download_bytes:   u64,

    pub tcp_ratio:        f64,
    pub udp_ratio:        f64,
    pub icmp_ratio:       f64,
}

impl FlowFeatures {
    pub const FEATURE_DIM: usize = 9;

    /// Returns the normalised numeric vector fed into the model.
    /// Field order is a fixed contract — do not reorder.
    pub fn to_vec(&self) -> Vec<f64> {
        let ul_ratio = if self.upload_bytes + self.download_bytes > 0 {
            self.upload_bytes as f64 / (self.upload_bytes + self.download_bytes) as f64
        } else {
            0.5
        };
        vec![
            (self.pps).ln_1p(),
            (self.bps / 1_000.0).ln_1p(),
            self.avg_pkt_bytes / 1500.0,
            (self.unique_dst_ips as f64).ln_1p(),
            (self.unique_dst_ports as f64).ln_1p(),
            ul_ratio,
            self.tcp_ratio,
            self.udp_ratio,
            self.icmp_ratio,
        ]
    }
}

/// Represents the IP address family for a Flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IpAddrType {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

/// The common representation of a network flow decoupled from NetFlow v9 or IPFIX structures.
/// Packed efficiently for in-memory use and aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NormalizedFlow {
    /// Unix timestamp in seconds for the start or arrival of the flow window.
    pub timestamp: u64,
    /// Exporter IP addressing the entity sending this telemetry
    pub exporter_ip: Ipv4Addr, 
    /// Source IP Address
    pub src_ip: IpAddrType,
    /// Destination IP Address
    pub dst_ip: IpAddrType,
    /// Source Port
    pub src_port: u16,
    /// Destination Port
    pub dst_port: u16,
    /// IP Protocol (e.g., 6 for TCP, 17 for UDP)
    pub protocol: u8,
    /// Number of bytes transferred in this flow window
    pub bytes: u64,
    /// Number of packets transferred in this flow window
    pub packets: u64,
    /// Autonomous System Number (ASN) of the source (if available)
    pub src_asn: u32,
    /// Autonomous System Number (ASN) of the destination (if available)
    pub dst_asn: u32,
    /// Ingress logical interface
    pub ingress_interface: u32,
    /// Egress logical interface
    pub egress_interface: u32,
    /// Cumulative TCP flags (if applicable)
    pub tcp_flags: u8,
}
