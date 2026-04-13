use ahash::RandomState;
use flow_types::{IpAddrType, NormalizedFlow};
use std::collections::HashMap;

/// The key used to aggregate flows in fixed time windows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AggregationKey {
    pub src_ip: IpAddrType,
    pub dst_ip: IpAddrType,
    pub dst_port: u16,
    pub protocol: u8,
}

/// The accumulated metrics for a given AggregationKey
#[derive(Debug, Default)]
pub struct AggregatedMetrics {
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
}

/// A highly-performant aggregation map owned entirely by a single Worker thread.
/// At the end of every 1-second window, this map is swapped and sent to the Exporter thread.
pub struct ThreadLocalAggregator {
    map: HashMap<AggregationKey, AggregatedMetrics, RandomState>,
}

impl ThreadLocalAggregator {
    pub fn new() -> Self {
        Self {
            // Pre-allocate a reasonable capacity to avoid re-allocations on the hot path
            map: HashMap::with_capacity_and_hasher(16384, RandomState::new()),
        }
    }

    /// Aggregates a normalized flow into the current window
    #[inline]
    pub fn aggregate(&mut self, flow: &NormalizedFlow) {
        let key = AggregationKey {
            src_ip: flow.src_ip,
            dst_ip: flow.dst_ip,
            dst_port: flow.dst_port,
            protocol: flow.protocol,
        };

        let metrics = self.map.entry(key).or_insert_with(AggregatedMetrics::default);
        metrics.packets += flow.packets;
        metrics.bytes += flow.bytes;
        metrics.flow_count += 1;
    }

    /// Retries the current map, swapping it with a an empty pre-allocated one
    pub fn flush_window(&mut self) -> HashMap<AggregationKey, AggregatedMetrics, RandomState> {
        let mut new_map = HashMap::with_capacity_and_hasher(self.map.capacity(), RandomState::new());
        std::mem::swap(&mut self.map, &mut new_map);
        new_map
    }
}
