# Task 9.1 — Feature Extractor (janela agregada → vetor ML)

## Goal

Extrair um vetor de features por IP por janela de tempo a partir dos dados já agregados
pelo `aggregator`. Este vetor é a entrada para o modelo de Isolation Forest (task 9.2).

A extração acontece no pipeline existente, após o flush do aggregator e antes do insert
no ClickHouse — reutiliza o mesmo `HashMap<AggregationKey, AggregatedMetrics>` já presente.

---

## Files to create/modify

| File | Action |
|------|--------|
| `flow-types/src/lib.rs` | MODIFY — add `FlowFeatures` struct |
| `collector-core/src/features.rs` | CREATE — extractor logic |
| `collector-core/src/main.rs` | MODIFY — call extractor after aggregator flush |

---

## `FlowFeatures` struct

```rust
// flow-types/src/lib.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlowFeatures {
    pub exporter_ip: String,
    pub src_ip:      String,
    pub window_ts:   u32,    // Unix timestamp do início da janela

    // Volume
    pub bytes_total:    u64,
    pub packets_total:  u64,
    pub flows_total:    u64,
    pub avg_pkt_bytes:  f64,

    // Ritmo
    pub pps:            f64,   // packets / window_secs
    pub bps:            f64,   // bytes / window_secs

    // Diversidade de destino
    pub unique_dst_ips:   u32,
    pub unique_dst_ports: u32,

    // Direção (upload vs download — calculado em relação ao exporter)
    pub upload_bytes:   u64,   // bytes onde src_ip == assinante envia
    pub download_bytes: u64,   // bytes onde src_ip == assinante recebe

    // Protocolos
    pub tcp_ratio:  f64,   // fração de bytes em TCP
    pub udp_ratio:  f64,
    pub icmp_ratio: f64,
}

impl FlowFeatures {
    /// Vetor numérico normalizado para entrada no modelo.
    /// Ordem dos campos é contrato fixo — não reordenar.
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

    pub const FEATURE_DIM: usize = 9;
}
```

---

## `features.rs` — extractor

Recebe o `HashMap` do aggregator e produz um `Vec<FlowFeatures>`, um por IP por exporter.
Precisa de um segundo passo para calcular `download_bytes` (dst_ip perspective), que requer
varrer o mapa procurando pares onde `dst_ip == src_ip` de outra entrada.

```rust
use std::collections::HashMap;
use aggregator::{AggregationKey, AggregatedMetrics};
use flow_types::{FlowFeatures, IpAddrType};
use ahash::RandomState;

const WINDOW_SECS: f64 = 1.0; // aggregator janela de 1s

pub fn extract(
    window_ts: u32,
    map: &HashMap<AggregationKey, AggregatedMetrics, RandomState>,
) -> Vec<FlowFeatures> {
    // Aggregate per (exporter_ip, src_ip)
    // Track: bytes, packets, flows, dst_ips (HashSet), dst_ports (HashSet),
    //        bytes_per_proto (tcp/udp/icmp), upload_bytes
    // Second pass: for each (exporter, src_ip) find download_bytes
    //              by looking for entries where dst_ip == src_ip

    // ... implementation details below ...
}
```

### Build strategy

```
for each (key, metrics) in map:
    if key.src_ip is IPv4:
        entry = per_ip_map[(key.exporter_ip, key.src_ip)]
        entry.bytes       += metrics.bytes
        entry.packets     += metrics.packets
        entry.flows       += metrics.flow_count
        entry.dst_ips.insert(key.dst_ip)
        entry.dst_ports.insert(key.dst_port)
        entry.upload_bytes += metrics.bytes   // sender perspective
        match key.protocol:
            6  → entry.tcp_bytes  += metrics.bytes
            17 → entry.udp_bytes  += metrics.bytes
            1  → entry.icmp_bytes += metrics.bytes

// Download pass: bytes where THIS IP is dst_ip
for each (key, metrics) in map:
    if key.dst_ip is IPv4:
        if let Some(entry) = per_ip_map.get_mut((key.exporter_ip, key.dst_ip)):
            entry.download_bytes += metrics.bytes

// Convert to FlowFeatures
for (exporter_ip, src_ip), entry in per_ip_map:
    features.push(FlowFeatures {
        pps: entry.packets as f64 / WINDOW_SECS,
        bps: entry.bytes as f64 / WINDOW_SECS,
        avg_pkt_bytes: entry.bytes as f64 / entry.packets.max(1) as f64,
        unique_dst_ips:   entry.dst_ips.len() as u32,
        unique_dst_ports: entry.dst_ports.len() as u32,
        tcp_ratio:  entry.tcp_bytes  as f64 / entry.bytes.max(1) as f64,
        udp_ratio:  entry.udp_bytes  as f64 / entry.bytes.max(1) as f64,
        icmp_ratio: entry.icmp_bytes as f64 / entry.bytes.max(1) as f64,
        upload_bytes:   entry.upload_bytes,
        download_bytes: entry.download_bytes,
        ...
    })
```

---

## `main.rs` changes

```rust
// After aggregator flush, before/alongside ClickHouse insert:
let features = features::extract(now, &map);

if !features.is_empty() {
    // Send to ML inference channel (task 9.2)
    let _ = ml_tx.try_send(features); // non-blocking, drop if channel full
}
```

Use `flume::bounded(100)` para o canal — se o modelo estiver lento, descarta janelas
antigas sem bloquear o hot path. Features de rede não são idempotentes mas a continuidade
do detector é mais importante que processar cada janela.

---

## Acceptance criteria

- [ ] `cargo build` passes
- [ ] `FlowFeatures::to_vec()` retorna vetor de dimensão `FEATURE_DIM = 9`
- [ ] `ln_1p` aplicado em valores de escala alta (pps, bps) — sem overflow
- [ ] Extrator não aloca mais de O(unique_ips × 100 bytes) por janela
- [ ] Canal `ml_tx` com `try_send` — hot path nunca bloqueia
- [ ] IPv6 ignorado nesta fase (mesmo tratamento do resto do sistema)
