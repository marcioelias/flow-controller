use aggregator::SlicedMap;
use ahash::RandomState;
use flow_types::{FlowFeatures, IpAddrType};
use std::collections::{HashMap, HashSet};

const WINDOW_SECS: f64 = 1.0;

#[derive(Default)]
struct IpEntry {
    bytes: u64,
    packets: u64,
    flows: u64,
    upload_bytes: u64,
    download_bytes: u64,
    tcp_bytes: u64,
    udp_bytes: u64,
    icmp_bytes: u64,
    dst_ips: HashSet<u32, RandomState>,
    dst_ports: HashSet<u16, RandomState>,
}

pub fn extract(map: &SlicedMap) -> Vec<FlowFeatures> {
    // Key: (slice_sec, exporter_ip_u32, src_ipv4_u32) — a janela do ML é o
    // segundo REAL da fatia (task 13.11): antes o lote drenado inteiro era
    // tratado como 1s, e rajadas de expiração inflavam pps/bps das features
    // (292k pps num link de 100 Mbps). ahash como o resto do pipeline.
    let mut per_ip: HashMap<(u32, u32, u32), IpEntry, RandomState> =
        HashMap::with_hasher(RandomState::new());

    // Pass 1 — sender perspective (src_ip sent these bytes)
    for ((sec, key), metrics) in map.iter() {
        let (exporter_u32, src_u32) = match (key.exporter_ip, key.src_ip) {
            (exp, IpAddrType::V4(src)) => (
                u32::from_be_bytes(exp.octets()),
                u32::from_be_bytes(src.octets()),
            ),
            _ => continue, // skip IPv6 entries
        };

        let dst_u32 = match key.dst_ip {
            IpAddrType::V4(d) => u32::from_be_bytes(d.octets()),
            _ => continue,
        };

        let entry = per_ip.entry((*sec, exporter_u32, src_u32)).or_default();
        entry.bytes += metrics.bytes;
        entry.packets += metrics.packets;
        entry.flows += metrics.flow_count;
        entry.upload_bytes += metrics.bytes;
        entry.dst_ips.insert(dst_u32);
        entry.dst_ports.insert(key.dst_port);

        match key.protocol {
            6 => entry.tcp_bytes += metrics.bytes,
            17 => entry.udp_bytes += metrics.bytes,
            1 => entry.icmp_bytes += metrics.bytes,
            _ => {}
        }
    }

    // Pass 2 — receiver perspective: where src_ip is dst_ip of another entry
    for ((sec, key), metrics) in map.iter() {
        let (exporter_u32, dst_u32) = match (key.exporter_ip, key.dst_ip) {
            (exp, IpAddrType::V4(dst)) => (
                u32::from_be_bytes(exp.octets()),
                u32::from_be_bytes(dst.octets()),
            ),
            _ => continue,
        };

        if let Some(entry) = per_ip.get_mut(&(*sec, exporter_u32, dst_u32)) {
            entry.download_bytes += metrics.bytes;
        }
    }

    per_ip
        .into_iter()
        .map(|((sec, exporter_u32, src_u32), e)| {
            let bytes_f = e.bytes.max(1) as f64;
            let pkts_f = e.packets.max(1) as f64;

            FlowFeatures {
                exporter_ip: std::net::Ipv4Addr::from(exporter_u32).to_string(),
                src_ip: std::net::Ipv4Addr::from(src_u32).to_string(),
                window_ts: sec,
                bytes_total: e.bytes,
                packets_total: e.packets,
                flows_total: e.flows,
                avg_pkt_bytes: e.bytes as f64 / pkts_f,
                pps: pkts_f / WINDOW_SECS,
                bps: e.bytes as f64 / WINDOW_SECS,
                unique_dst_ips: e.dst_ips.len() as u32,
                unique_dst_ports: e.dst_ports.len() as u32,
                upload_bytes: e.upload_bytes,
                download_bytes: e.download_bytes,
                tcp_ratio: e.tcp_bytes as f64 / bytes_f,
                udp_ratio: e.udp_bytes as f64 / bytes_f,
                icmp_ratio: e.icmp_bytes as f64 / bytes_f,
            }
        })
        .collect()
}
