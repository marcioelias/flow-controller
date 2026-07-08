use flume::Receiver;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const WORKER_COUNT: usize = 4;
const QUEUE_CAPACITY: usize = 1_000_000;

#[allow(dead_code)]
struct PacketPayload {
    pub exporter_ip: std::net::IpAddr,
    pub data: Vec<u8>,
}

fn worker_loop(_id: usize, rx: Receiver<PacketPayload>, processed_counter: Arc<AtomicUsize>) {
    while let Ok(_payload) = rx.recv() {
        // Simulate minor processing overhead
        processed_counter.fetch_add(1, Ordering::Relaxed);
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Flow Collector Benchmark");

    let mut worker_senders = Vec::new();
    let mut worker_threads = Vec::new();
    let processed_counter = Arc::new(AtomicUsize::new(0));

    for worker_id in 0..WORKER_COUNT {
        let (tx, rx) = flume::bounded(QUEUE_CAPACITY);
        worker_senders.push(tx);

        let counter_clone = processed_counter.clone();
        let handle = thread::spawn(move || worker_loop(worker_id, rx, counter_clone));

        worker_threads.push(handle);
    }

    let dropped_packets = Arc::new(AtomicUsize::new(0));

    // A simulated packet dispatcher
    let dropped_clone = dropped_packets.clone();
    let dispatcher_thread = thread::spawn(move || {
        let dummy_ip = std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let dummy_data = vec![0u8; 1400]; // typical MTU-sized Netflow payload

        let start = Instant::now();
        let target_packets = 5_000_000;

        for i in 0..target_packets {
            let worker_idx = i % WORKER_COUNT;
            let payload = PacketPayload {
                exporter_ip: dummy_ip,
                data: dummy_data.clone(), // Heavy allocation here mimicking standard Vec behavior
            };

            if let Err(_) = worker_senders[worker_idx].try_send(payload) {
                dropped_clone.fetch_add(1, Ordering::Relaxed);
            }
        }

        let elapsed = start.elapsed();
        let rate = (target_packets as f64) / elapsed.as_secs_f64();
        tracing::info!("Dispatcher finished sending {} packets in {:?}", target_packets, elapsed);
        tracing::info!("Simulated throughput: {:.2} packets/sec", rate);
    });

    dispatcher_thread.join().unwrap();
    
    // Wait for queues to drain a bit
    thread::sleep(Duration::from_millis(500));
    
    let processed = processed_counter.load(Ordering::SeqCst);
    let dropped = dropped_packets.load(Ordering::SeqCst);
    tracing::info!("Total processed: {}", processed);
    tracing::info!("Total dropped: {}", dropped);

    Ok(())
}
