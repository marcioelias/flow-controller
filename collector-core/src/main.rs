use clickhouse_exporter::{ClickhouseExporter, NetworkFlowV4Row, NetworkFlowV6Row};
use flume::{Receiver, Sender};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use aggregator::{AggregatedMetrics, AggregationKey, ThreadLocalAggregator};
use netflow_parser::parse_packet;
use template_cache::ThreadLocalTemplateCache;

const UDP_BUFFER_SIZE: usize = 65536;
const WORKER_COUNT: usize = 4;
const QUEUE_CAPACITY: usize = 1_000_000;
const FLUSH_INTERVAL_SECS: u64 = 5;

struct PacketPayload {
    pub exporter_ip: std::net::IpAddr,
    pub data: Vec<u8>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting High-Performance Flow Collector");

    // Start Tokio runtime for the ClickHouse Exporter
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    
    let clickhouse_url = "http://localhost:8123";
    let exporter_client = Arc::new(ClickhouseExporter::new(clickhouse_url));
    rt.block_on(exporter_client.setup_tables())?;
    tracing::info!("Clickhouse tables validated successfully!");

    let (export_tx, export_rx) = flume::bounded(100);

    // Spawn async ClickHouse Exporter Task
    let exporter_clone = Arc::clone(&exporter_client);
    rt.spawn(async move {
        tracing::info!("Clickhouse Exporter background task started.");
        while let Ok(map) = export_rx.recv_async().await {
            let mut v4_batch = Vec::new();
            let mut v6_batch = Vec::new();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32;

            let map: std::collections::HashMap<AggregationKey, AggregatedMetrics, ahash::RandomState> = map;

            for (key, metrics) in map {
                match (key.src_ip, key.dst_ip) {
                    (flow_types::IpAddrType::V4(src_ip), flow_types::IpAddrType::V4(dst_ip)) => {
                        v4_batch.push(NetworkFlowV4Row {
                            timestamp: now,
                            src_ip,
                            dst_ip,
                            dst_port: key.dst_port,
                            protocol: key.protocol,
                            packets: metrics.packets,
                            bytes: metrics.bytes,
                            flow_count: metrics.flow_count,
                        });
                    }
                    (flow_types::IpAddrType::V6(src_ip), flow_types::IpAddrType::V6(dst_ip)) => {
                        v6_batch.push(NetworkFlowV6Row {
                            timestamp: now,
                            src_ip,
                            dst_ip,
                            dst_port: key.dst_port,
                            protocol: key.protocol,
                            packets: metrics.packets,
                            bytes: metrics.bytes,
                            flow_count: metrics.flow_count,
                        });
                    }
                    _ => {} // Mixed IPs (impossible organically)
                }
            }

            if let Err(e) = exporter_clone.insert_batch(&v4_batch, &v6_batch).await {
                tracing::error!("Failed to insert batch to ClickHouse: {}", e);
            } else {
                tracing::info!("Inserted batches (v4: {}, v6: {}) to ClickHouse.", v4_batch.len(), v6_batch.len());
            }
        }
    });

    // Pre-allocate channels for N workers
    let mut worker_senders = Vec::new();
    let mut worker_threads = Vec::new();

    // Spawn Worker Threads
    for worker_id in 0..WORKER_COUNT {
        let (tx, rx) = flume::bounded(QUEUE_CAPACITY);
        worker_senders.push(tx);

        let exp_tx = export_tx.clone();
        let handle = thread::Builder::new()
            .name(format!("worker-{}", worker_id))
            .spawn(move || worker_loop(worker_id, rx, exp_tx))?;

        worker_threads.push(handle);
    }

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    
    if let Err(e) = socket.set_recv_buffer_size(32 * 1024 * 1024) { 
        tracing::warn!("Could not set optimal UDP recv buffer size: {}", e);
    }
    
    let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 2055);
    socket.bind(&addr.into())?;
    let udp_socket: UdpSocket = socket.into();
    
    tracing::info!("Listening for NetFlow/IPFIX on UDP port 2055 with {} workers", WORKER_COUNT);

    let dropped_packets = Arc::new(AtomicUsize::new(0));

    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        match udp_socket.recv_from(&mut buf) {
            Ok((size, src_addr)) => {
                let worker_idx = match src_addr.ip() {
                    std::net::IpAddr::V4(ipv4) => {
                        (u32::from_be_bytes(ipv4.octets()) as usize) % WORKER_COUNT
                    }
                    std::net::IpAddr::V6(ipv6) => {
                        (u128::from_be_bytes(ipv6.octets()) as usize) % WORKER_COUNT
                    }
                };

                let payload = PacketPayload {
                    exporter_ip: src_addr.ip(),
                    data: buf[..size].to_vec(),
                };

                if let Err(flume::TrySendError::Full(_)) = worker_senders[worker_idx].try_send(payload) {
                    dropped_packets.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(e) => tracing::error!("UDP recv error: {}", e),
        }
    }
}

fn worker_loop(
    id: usize,
    rx: Receiver<PacketPayload>,
    export_tx: Sender<std::collections::HashMap<AggregationKey, AggregatedMetrics, ahash::RandomState>>,
) {
    tracing::info!("Worker {} started", id);

    let mut templates = ThreadLocalTemplateCache::new();
    let mut aggregator = ThreadLocalAggregator::new();
    let mut last_flush = Instant::now();

    while let Ok(payload) = rx.recv() {
        if let Ok(parsed_flows) = parse_packet(&payload.data, &mut templates, payload.exporter_ip) {
            for flow in parsed_flows {
                aggregator.aggregate(&flow);
            }
        }
        
        // 5 Second flush
        if last_flush.elapsed() >= Duration::from_secs(FLUSH_INTERVAL_SECS) {
            let map = aggregator.flush_window();
            if !map.is_empty() {
                let _ = export_tx.try_send(map);
            }
            last_flush = Instant::now();
        }
    }
}
