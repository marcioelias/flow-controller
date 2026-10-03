use ahash::RandomState;
use flow_types::{IpAddrType, NormalizedFlow};
use std::collections::HashMap;

/// The key used to aggregate flows in fixed time windows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AggregationKey {
    pub exporter_ip: std::net::Ipv4Addr,
    pub src_ip: IpAddrType,
    pub dst_ip: IpAddrType,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub src_asn: u32,
    pub dst_asn: u32,
    /// Router-reported direction (IE 61): flows in opposite directions must
    /// not merge, otherwise in/out split is lost
    pub direction: u8,
    /// Post-NAT translation (task 17.8): different translations must not merge
    pub nat_src_ip: Option<IpAddrType>,
    pub nat_dst_ip: Option<IpAddrType>,
    pub nat_src_port: u16,
    pub nat_dst_port: u16,
}

/// The accumulated metrics for a given AggregationKey
#[derive(Debug, Default)]
pub struct AggregatedMetrics {
    pub packets: u64,
    pub bytes: u64,
    pub flow_count: u64,
}

/// Slices are keyed by the unix second the traffic actually happened in,
/// not by arrival time.
pub type SlicedMap = HashMap<(u32, AggregationKey), AggregatedMetrics, RandomState>;

/// How far back a flow may be spread. Covers the active timeout of any sane
/// exporter (60–120s typical); also bounds the per-flow work and protects
/// against exporters with a broken clock.
pub const MAX_SPREAD_SECS: u32 = 300;

/// Resolve the [first, last] slice seconds for a flow. Flows without
/// timestamps (or with nonsense ones) collapse to the current second.
#[inline]
fn slice_bounds(start_ms: u64, end_ms: u64, now_secs: u32) -> (u32, u32) {
    if start_ms == 0 || end_ms < start_ms {
        return (now_secs, now_secs);
    }
    let oldest = now_secs.saturating_sub(MAX_SPREAD_SECS);
    // Exporter clock ahead of ours → clamp to now; behind the spread
    // horizon → clamp to the horizon edge (bytes are preserved either way)
    let s1 = ((end_ms / 1000) as u32).clamp(oldest, now_secs);
    let s0 = ((start_ms / 1000) as u32).clamp(oldest, s1);
    (s0, s1)
}

/// A highly-performant aggregation map owned entirely by a single Worker thread.
/// At the end of every 1-second window, this map is swapped and sent to the Exporter thread.
pub struct ThreadLocalAggregator {
    map: SlicedMap,
}

impl Default for ThreadLocalAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadLocalAggregator {
    pub fn new() -> Self {
        Self {
            // Pre-allocate a reasonable capacity to avoid re-allocations on the hot path
            map: HashMap::with_capacity_and_hasher(16384, RandomState::new()),
        }
    }

    /// Aggregates a normalized flow, spreading bytes/packets evenly over the
    /// seconds the flow actually spanned (task 13.2). A 20s upload becomes a
    /// 20s ramp in the stored data instead of a one-second spike at export.
    #[inline]
    pub fn aggregate(&mut self, flow: &NormalizedFlow, now_secs: u32) {
        let key = AggregationKey {
            exporter_ip: flow.exporter_ip,
            src_ip: flow.src_ip,
            dst_ip: flow.dst_ip,
            src_port: flow.src_port,
            dst_port: flow.dst_port,
            protocol: flow.protocol,
            src_asn: flow.src_asn,
            dst_asn: flow.dst_asn,
            direction: flow.direction,
            nat_src_ip: flow.nat_src_ip,
            nat_dst_ip: flow.nat_dst_ip,
            nat_src_port: flow.nat_src_port,
            nat_dst_port: flow.nat_dst_port,
        };

        let (s0, s1) = slice_bounds(flow.start_ms, flow.end_ms, now_secs);
        let n = (s1 - s0 + 1) as u64;

        if n == 1 {
            let metrics = self.map.entry((s1, key)).or_default();
            metrics.packets += flow.packets;
            metrics.bytes += flow.bytes;
            metrics.flow_count += 1;
            return;
        }

        // Even split, remainder on the last slice — no byte created or lost
        let bytes_per = flow.bytes / n;
        let pkts_per = flow.packets / n;
        for sec in s0..=s1 {
            let metrics = self.map.entry((sec, key)).or_default();
            metrics.bytes += bytes_per;
            metrics.packets += pkts_per;
            if sec == s0 {
                // The flow is one flow no matter how many slices it spans
                metrics.flow_count += 1;
            }
            if sec == s1 {
                metrics.bytes += flow.bytes % n;
                metrics.packets += flow.packets % n;
            }
        }
    }

    /// Retrieves the current map, swapping it with an empty pre-allocated one
    pub fn flush_window(&mut self) -> SlicedMap {
        let mut new_map =
            HashMap::with_capacity_and_hasher(self.map.capacity(), RandomState::new());
        std::mem::swap(&mut self.map, &mut new_map);
        new_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    const NOW: u32 = 1_700_000_100;

    fn flow(bytes: u64, packets: u64, start_ms: u64, end_ms: u64) -> NormalizedFlow {
        NormalizedFlow {
            timestamp: NOW as u64,
            exporter_ip: Ipv4Addr::new(10, 0, 0, 1),
            src_ip: IpAddrType::V4(Ipv4Addr::new(192, 168, 0, 2)),
            dst_ip: IpAddrType::V4(Ipv4Addr::new(1, 1, 1, 1)),
            src_port: 40000,
            dst_port: 443,
            protocol: 6,
            bytes,
            packets,
            src_asn: 0,
            dst_asn: 0,
            ingress_interface: 0,
            egress_interface: 0,
            tcp_flags: 0,
            sampling_rate: 1,
            direction: flow_types::DIRECTION_EGRESS,
            start_ms,
            end_ms,
            nat_src_ip: None,
            nat_dst_ip: None,
            nat_src_port: 0,
            nat_dst_port: 0,
        }
    }

    fn totals(map: &SlicedMap) -> (u64, u64, u64) {
        map.values().fold((0, 0, 0), |acc, m| {
            (acc.0 + m.bytes, acc.1 + m.packets, acc.2 + m.flow_count)
        })
    }

    #[test]
    fn no_timestamps_lands_on_now() {
        let mut agg = ThreadLocalAggregator::new();
        agg.aggregate(&flow(1000, 10, 0, 0), NOW);
        let map = agg.flush_window();
        assert_eq!(map.len(), 1);
        let ((sec, _), m) = map.iter().next().unwrap();
        assert_eq!(*sec, NOW);
        assert_eq!(m.bytes, 1000);
        assert_eq!(m.flow_count, 1);
    }

    #[test]
    fn spread_preserves_totals_with_remainder() {
        let mut agg = ThreadLocalAggregator::new();
        // 10s span, 1003 bytes → 100/slice + 3 on the last
        let start = (NOW as u64 - 9) * 1000;
        let end = NOW as u64 * 1000;
        agg.aggregate(&flow(1003, 23, start, end), NOW);
        let map = agg.flush_window();
        assert_eq!(map.len(), 10);
        assert_eq!(totals(&map), (1003, 23, 1));
        let last = map.get(&(NOW, key_of(&map))).unwrap();
        assert_eq!(last.bytes, 100 + 3);
    }

    #[test]
    fn future_end_clamps_to_now() {
        let mut agg = ThreadLocalAggregator::new();
        let start = (NOW as u64 - 2) * 1000;
        let end = (NOW as u64 + 60) * 1000; // exporter clock ahead
        agg.aggregate(&flow(300, 3, start, end), NOW);
        let map = agg.flush_window();
        assert_eq!(map.len(), 3); // NOW-2 ..= NOW
        assert_eq!(totals(&map), (300, 3, 1));
        assert!(map.keys().all(|(sec, _)| *sec <= NOW));
    }

    #[test]
    fn ancient_flow_clamps_to_horizon() {
        let mut agg = ThreadLocalAggregator::new();
        let start = (NOW as u64 - 10_000) * 1000;
        let end = (NOW as u64 - 9_000) * 1000;
        agg.aggregate(&flow(500, 5, start, end), NOW);
        let map = agg.flush_window();
        // Entirely before the horizon → single slice at the horizon edge
        assert_eq!(map.len(), 1);
        let ((sec, _), m) = map.iter().next().unwrap();
        assert_eq!(*sec, NOW - MAX_SPREAD_SECS);
        assert_eq!(m.bytes, 500);
    }

    #[test]
    fn sub_second_flow_single_slice() {
        let mut agg = ThreadLocalAggregator::new();
        let ms = NOW as u64 * 1000;
        agg.aggregate(&flow(800, 8, ms + 100, ms + 900), NOW);
        let map = agg.flush_window();
        assert_eq!(map.len(), 1);
        assert_eq!(totals(&map), (800, 8, 1));
    }

    fn key_of(map: &SlicedMap) -> AggregationKey {
        *map.keys().next().map(|(_, k)| k).unwrap()
    }
}
