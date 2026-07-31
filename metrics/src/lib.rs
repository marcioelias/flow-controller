use prometheus::{IntCounter, IntGauge, Registry};

pub struct CollectorMetrics {
    pub registry: Registry,
    /// UDP packets accepted from whitelisted exporters
    pub packets_received: IntCounter,
    /// UDP packets rejected by the exporter whitelist
    pub packets_blocked: IntCounter,
    /// UDP packets dropped because a worker queue was full
    pub packets_dropped: IntCounter,
    /// Packets that failed NetFlow/IPFIX parsing
    pub parse_errors: IntCounter,
    /// Flow records successfully decoded
    pub flows_decoded: IntCounter,
    /// Aggregation windows dropped because the export queue was full
    pub export_windows_dropped: IntCounter,
    /// Feature batches dropped because the ML queue was full
    pub ml_windows_dropped: IntCounter,
    /// ClickHouse insert batches that failed permanently
    pub clickhouse_insert_errors: IntCounter,
    /// Rows successfully inserted into ClickHouse
    pub clickhouse_rows_inserted: IntCounter,
    pub template_cache_size: IntGauge,
    /// Current depth of the export queue (windows waiting for insert)
    pub export_queue_depth: IntGauge,
    /// DEPRECATED: same value as packets_received — kept so existing
    /// dashboards reading flows_received_total keep working
    pub flows_received: IntCounter,
}

impl Default for CollectorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

fn counter(registry: &Registry, name: &str, help: &str) -> IntCounter {
    let c = IntCounter::new(name, help).unwrap();
    registry.register(Box::new(c.clone())).unwrap();
    c
}

fn gauge(registry: &Registry, name: &str, help: &str) -> IntGauge {
    let g = IntGauge::new(name, help).unwrap();
    registry.register(Box::new(g.clone())).unwrap();
    g
}

impl CollectorMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();

        let packets_received = counter(
            &registry,
            "packets_received_total",
            "UDP packets accepted from whitelisted exporters",
        );
        let packets_blocked = counter(
            &registry,
            "packets_blocked_total",
            "UDP packets rejected by the exporter whitelist",
        );
        let packets_dropped = counter(
            &registry,
            "packets_dropped_total",
            "UDP packets dropped because a worker queue was full",
        );
        let parse_errors = counter(
            &registry,
            "parse_errors_total",
            "Packets that failed NetFlow/IPFIX parsing",
        );
        let flows_decoded = counter(
            &registry,
            "flows_decoded_total",
            "Total raw flow records successfully decoded",
        );
        let export_windows_dropped = counter(
            &registry,
            "export_windows_dropped_total",
            "Aggregation windows dropped because the export queue was full",
        );
        let ml_windows_dropped = counter(
            &registry,
            "ml_windows_dropped_total",
            "Feature batches dropped because the ML queue was full",
        );
        let clickhouse_insert_errors = counter(
            &registry,
            "clickhouse_insert_errors_total",
            "ClickHouse insert batches that failed permanently",
        );
        let clickhouse_rows_inserted = counter(
            &registry,
            "clickhouse_rows_inserted_total",
            "Rows successfully inserted into ClickHouse",
        );
        let template_cache_size = gauge(
            &registry,
            "template_cache_size",
            "Current number of templates across all workers",
        );
        let export_queue_depth = gauge(
            &registry,
            "export_queue_depth",
            "Aggregation windows waiting in the export queue",
        );
        let flows_received = counter(
            &registry,
            "flows_received_total",
            "DEPRECATED: same as packets_received_total (kept for old dashboards)",
        );

        Self {
            registry,
            packets_received,
            packets_blocked,
            packets_dropped,
            parse_errors,
            flows_decoded,
            export_windows_dropped,
            ml_windows_dropped,
            clickhouse_insert_errors,
            clickhouse_rows_inserted,
            template_cache_size,
            export_queue_depth,
            flows_received,
        }
    }
}
