# Task 9.2 — Isolation Forest (modelo ML + pipeline de treino)

## Goal

Treinar e servir um modelo de Isolation Forest por exporter usando `linfa` e `linfa-trees`.
O modelo aprende o perfil de tráfego normal de cada assinante a partir das features extraídas
pela task 9.1 — sem supervisão e sem labels manuais. Anomalias acima do threshold disparam
eventos no sistema de alertas (task 6.x).

---

## Crate dependencies

```toml
# collector-core/Cargo.toml
linfa             = "0.7"
linfa-trees       = { version = "0.7", features = ["serde"] }  # includes IsolationForest
flume             = "0.11"                                       # bounded channel for features
serde_json        = "1"                                          # already present
```

> `linfa-trees` expõe `IsolationTree` e `IsolationForest`. Treinamento é síncrono
> (microsegundos para ~1000 samples) — roda em `tokio::task::spawn_blocking` para não
> bloquear o executor async.

---

## Files to create/modify

| File | Action |
|------|--------|
| `collector-core/src/ml_model.rs` | CREATE — model store + train + score |
| `collector-core/src/ml_runner.rs` | CREATE — inference loop consuming flume channel |
| `collector-core/src/main.rs` | MODIFY — init channel, spawn ml_runner |

---

## Architecture

```
[aggregator flush]
       │
       ▼  Vec<FlowFeatures>
  flume::bounded(100) ──────────────────────► ml_runner (tokio task)
       │                                              │
       │                                    for each FlowFeatures:
       │                                      score = model.predict(features)
       │                                      if score > threshold → alert
       │
       ▼
  (continues to ClickHouse insert — never blocked)
```

### Model lifecycle per exporter

```
Phase 1 (warm-up, 24h):  collect features → ring buffer, no alerts
Phase 2 (trained):       score each window → alert if anomalous
Phase 3 (retrain):       every 6h, rebuild model from ring buffer
                         new model replaces old atomically (RwLock swap)
```

---

## `ml_model.rs`

```rust
use std::collections::VecDeque;
use linfa::prelude::*;
use linfa_trees::IsolationForest;
use flow_types::FlowFeatures;

const WARMUP_SAMPLES:   usize = 5_000;   // ~83 min @ 1 IP/s, 60s windows
const RING_BUFFER_SIZE: usize = 50_000;  // ~14h of samples
const N_TREES:          usize = 100;
const ANOMALY_THRESHOLD: f64  = 0.65;   // score in [0,1]; higher = more anomalous

pub struct ExporterModel {
    pub model:    Option<IsolationForest<f64>>,
    pub buffer:   VecDeque<Vec<f64>>,
    pub n_scored: u64,
}

impl ExporterModel {
    pub fn new() -> Self {
        Self { model: None, buffer: VecDeque::new(), n_scored: 0 }
    }

    /// Add features to ring buffer; returns true if warm-up complete
    pub fn push(&mut self, v: Vec<f64>) -> bool {
        if self.buffer.len() >= RING_BUFFER_SIZE {
            self.buffer.pop_front();
        }
        self.buffer.push_back(v);
        self.buffer.len() >= WARMUP_SAMPLES
    }

    /// Train / retrain from current ring buffer.
    /// Call in spawn_blocking — CPU-bound but fast (~5ms for 50k×9).
    pub fn train(&mut self) -> anyhow::Result<()> {
        let data: Vec<f64> = self.buffer.iter().flatten().cloned().collect();
        let n = self.buffer.len();
        let dataset = Dataset::from(ndarray::Array2::from_shape_vec(
            (n, FlowFeatures::FEATURE_DIM), data
        )?);
        self.model = Some(
            IsolationForest::params()
                .num_trees(N_TREES)
                .fit(&dataset)?
        );
        Ok(())
    }

    /// Returns anomaly score in [0,1]. None if model not ready.
    pub fn score(&mut self, v: &[f64]) -> Option<f64> {
        let m = self.model.as_ref()?;
        self.n_scored += 1;
        let row = ndarray::Array2::from_shape_vec(
            (1, FlowFeatures::FEATURE_DIM),
            v.to_vec()
        ).ok()?;
        let pred = m.predict(&row);
        // linfa IsolationForest returns score: higher = more anomalous
        Some(pred[0])
    }

    pub fn is_ready(&self) -> bool {
        self.model.is_some()
    }

    pub fn threshold() -> f64 {
        ANOMALY_THRESHOLD
    }
}
```

---

## `ml_runner.rs`

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use flume::Receiver;
use flow_types::FlowFeatures;
use crate::alerts::{self, AlertEvent, AlertSeverity};
use crate::auth::AppState;
use crate::ml_model::ExporterModel;

// Retrain every N feature-batches processed (≈6h @ 1 batch/60s/exporter)
const RETRAIN_EVERY: u64 = 360;

pub async fn run_ml(
    state: Arc<AppState>,
    rx: Receiver<Vec<FlowFeatures>>,
) {
    tracing::info!("ML anomaly detector started");
    // Per-exporter models, guarded by RwLock for future multi-thread
    let models: Arc<RwLock<HashMap<String, ExporterModel>>> =
        Arc::new(RwLock::new(HashMap::new()));

    while let Ok(batch) = rx.recv_async().await {
        for feat in batch {
            let vec = feat.to_vec();
            let exporter = feat.exporter_ip.clone();

            // Get or create model for this exporter
            let mut map = models.write().unwrap();
            let m = map.entry(exporter.clone()).or_insert_with(ExporterModel::new);

            let warm = m.push(vec.clone());

            if !warm {
                continue; // still collecting warm-up samples
            }

            // Retrain periodically
            if m.n_scored % RETRAIN_EVERY == 0 {
                if let Err(e) = tokio::task::block_in_place(|| m.train()) {
                    tracing::warn!("ML retrain failed for {exporter}: {e}");
                    continue;
                }
                if m.n_scored == 0 {
                    // First train — score this sample now
                }
                tracing::debug!("ML model retrained for {exporter} ({} samples)", m.buffer.len());
            }

            // Score
            let score = match m.score(&vec) {
                Some(s) => s,
                None => continue,
            };

            drop(map); // release lock before async alert insert

            if score >= ExporterModel::threshold() {
                let severity = if score >= ExporterModel::threshold() + 0.15 {
                    AlertSeverity::Critical
                } else {
                    AlertSeverity::Warning
                };

                let message = format!(
                    "ML anomaly detected: score {:.3} (threshold {:.2}) — pps={:.0}, unique_dst_ports={:.0}",
                    score,
                    ExporterModel::threshold(),
                    feat.pps,
                    feat.unique_dst_ports,
                );

                let event = AlertEvent {
                    id: None,
                    rule_id: None,   // ML-generated, no user rule
                    exporter_ip: exporter.clone(),
                    src_ip: feat.src_ip.clone(),
                    alert_type: "ml_anomaly".to_string(),
                    severity,
                    message,
                    upload_bytes: Some(feat.upload_bytes as i64),
                    download_bytes: Some(feat.download_bytes as i64),
                    pps: Some(feat.pps),
                    avg_pkt_bytes: Some(feat.avg_pkt_bytes),
                    attack_ports: None,
                    notified: false,
                    bgp_announced: false,
                    created_at: None,
                };

                match alerts::insert_event(&state.db, &event).await {
                    Ok(id) => tracing::info!(
                        "ML alert #{id}: {} on {exporter} (score={score:.3})", feat.src_ip
                    ),
                    Err(e) => tracing::warn!("ML alert insert failed: {e}"),
                }
            }
        }
    }
}
```

---

## `main.rs` changes

```rust
mod ml_model;
mod ml_runner;

// After creating AppState, before binding the TCP listener:
let (ml_tx, ml_rx) = flume::bounded::<Vec<flow_types::FlowFeatures>>(100);

// Pass ml_tx to the aggregator flush closure
// Pass ml_rx to the runner
tokio::spawn(ml_runner::run_ml(state.clone(), ml_rx));
```

In the aggregator flush (where the HashMap is consumed):
```rust
let features = crate::features::extract(now_ts, &agg_map);
if !features.is_empty() {
    let _ = ml_tx.try_send(features); // drop if channel full — never blocks
}
```

---

## Database: new `alert_type` value

The existing `alert_events` table already supports arbitrary `alert_type` strings.
No migration needed — `ml_anomaly` is simply a new type value alongside
`upload_inversion` and `attack_signature`.

The frontend's `AlertEvents.vue` handles unknown types gracefully (shows `—` in
type-specific columns). A small update adds `ml_anomaly` display to the type badge.

---

## Model persistence (future — not in this task)

The trained model can be serialized to JSON via `serde` (linfa supports it with
`features = ["serde"]`). Persisting to SQLite blob allows warm restart without
re-collecting 24h of data. Defer to a follow-up task.

---

## Acceptance criteria

- [ ] `cargo build` passes with `linfa` and `linfa-trees` added
- [ ] `ExporterModel::train()` runs in `block_in_place` — does not block async executor
- [ ] Ring buffer stays bounded at `RING_BUFFER_SIZE` — no unbounded growth
- [ ] `ml_tx.try_send()` used — aggregator hot path never blocks
- [ ] `ml_anomaly` events appear in `/api/alerts/events`
- [ ] Anomaly score logged at DEBUG level; alert logged at INFO
- [ ] No `unsafe` code
