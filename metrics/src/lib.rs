use prometheus::core::Collector;
use prometheus::{GaugeVec, IntCounter, IntGauge, IntGaugeVec, Opts, Registry};

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
    /// Templates cached, labeled per worker (sum across workers for the total)
    pub template_cache_size: IntGaugeVec,
    /// Current depth of the export queue (windows waiting for insert)
    pub export_queue_depth: IntGauge,
    /// Sampling rate learned per exporter observation domain (1 = unsampled)
    pub exporter_sampling_rate: IntGaugeVec,
    /// 1 when an exporter reported both ingress and egress flows in the same
    /// window — summed totals would double-count that exporter
    pub exporter_bidirectional: IntGaugeVec,
    /// 1 quando a média de 5 min excede o max_bps da licença
    pub license_over_bps: IntGauge,
    /// 1 quando o excedente é sustentado (7d+) e as views analíticas bloqueiam
    pub license_degraded: IntGauge,
    /// Idade média flowEnd→chegada por exporter (s) — diagnóstico de active
    /// timeout mal configurado no roteador
    pub exporter_telemetry_lag: GaugeVec,
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
        let template_cache_size = IntGaugeVec::new(
            Opts::new(
                "template_cache_size",
                "Templates cached per worker (sum for the total)",
            ),
            &["worker"],
        )
        .unwrap();
        registry
            .register(Box::new(template_cache_size.clone()))
            .unwrap();
        let export_queue_depth = gauge(
            &registry,
            "export_queue_depth",
            "Aggregation windows waiting in the export queue",
        );
        let exporter_sampling_rate = IntGaugeVec::new(
            Opts::new(
                "exporter_sampling_rate",
                "Sampling rate learned per exporter observation domain (1 = unsampled)",
            ),
            &["exporter_ip", "domain"],
        )
        .unwrap();
        registry
            .register(Box::new(exporter_sampling_rate.clone()))
            .unwrap();
        let exporter_bidirectional = IntGaugeVec::new(
            Opts::new(
                "exporter_bidirectional",
                "1 when the exporter reports both ingress and egress (IE 61) in the same window",
            ),
            &["exporter_ip"],
        )
        .unwrap();
        registry
            .register(Box::new(exporter_bidirectional.clone()))
            .unwrap();

        let license_over_bps = gauge(
            &registry,
            "license_over_bps",
            "1 when 5-min average traffic exceeds the licensed max_bps",
        );
        let exporter_telemetry_lag = GaugeVec::new(
            Opts::new(
                "exporter_telemetry_lag_seconds",
                "Average flowEnd to arrival age per exporter (seconds)",
            ),
            &["exporter_ip"],
        )
        .unwrap();
        registry
            .register(Box::new(exporter_telemetry_lag.clone()))
            .unwrap();
        let license_degraded = gauge(
            &registry,
            "license_degraded",
            "1 when sustained license overage has degraded analytic views",
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
            exporter_sampling_rate,
            exporter_bidirectional,
            license_over_bps,
            license_degraded,
            exporter_telemetry_lag,
        }
    }
}

impl CollectorMetrics {
    /// Maior lag de telemetria entre os exporters (segundos)
    pub fn telemetry_lag_max(&self) -> f64 {
        self.exporter_telemetry_lag
            .collect()
            .iter()
            .flat_map(|mf| mf.get_metric().iter().map(|m| m.get_gauge().get_value()))
            .fold(0.0, f64::max)
    }

    /// Total de templates somando todos os workers (gauge é rotulado por worker)
    pub fn template_cache_total(&self) -> i64 {
        self.template_cache_size
            .collect()
            .iter()
            .flat_map(|mf| {
                mf.get_metric()
                    .iter()
                    .map(|m| m.get_gauge().get_value() as i64)
            })
            .sum()
    }
}
