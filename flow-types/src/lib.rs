use std::net::{Ipv4Addr, Ipv6Addr};

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
