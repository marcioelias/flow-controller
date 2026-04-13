use prometheus::{IntCounter, Registry, IntGauge};

pub struct CollectorMetrics {
    pub registry: Registry,
    pub flows_received: IntCounter,
    pub flows_decoded: IntCounter,
    pub packets_dropped: IntCounter,
    pub template_cache_size: IntGauge,
}

impl CollectorMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();
        
        // Exposing basic telemetry metrics
        let flows_received = IntCounter::new("flows_received_total", "Total Netflow packets received").unwrap();
        let flows_decoded = IntCounter::new("flows_decoded_total", "Total raw flow records successfully decoded").unwrap();
        let packets_dropped = IntCounter::new("packets_dropped_total", "Total UDP packets dropped due to backpressure").unwrap();
        let template_cache_size = IntGauge::new("template_cache_size", "Current number of templates across all workers").unwrap();

        registry.register(Box::new(flows_received.clone())).unwrap();
        registry.register(Box::new(flows_decoded.clone())).unwrap();
        registry.register(Box::new(packets_dropped.clone())).unwrap();
        registry.register(Box::new(template_cache_size.clone())).unwrap();

        Self {
            registry,
            flows_received,
            flows_decoded,
            packets_dropped,
            template_cache_size,
        }
    }
}
