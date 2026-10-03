/// Isolation Forest implementation — pure Rust, no external ML dependencies.
///
/// Algorithm: build N binary trees where each internal node randomly picks a feature
/// and a split value in [min, max] of that feature. Anomalies are isolated in fewer
/// splits on average → shorter path length → higher score.
///
/// Score formula: s(x,n) = 2^(−E[h(x)] / c(n))
///   where c(n) = 2*H(n-1) - (2*(n-1)/n)  (average path length in BST of size n)
///   and H(i) = ln(i) + 0.5772156649 (Euler–Mascheroni)
use rand::Rng;
use std::collections::VecDeque;

pub const WARMUP_SAMPLES: usize = 5_000;
/// Baseline must span at least an hour before scoring (task 17.10 R-03)
pub const WARMUP_SECS: u64 = 3_600;
/// Percentile thresholds never go below this score (R-04)
pub const MIN_THRESHOLD: f64 = 0.6;
const HOUR_RESERVOIR: usize = 2_000;
const HOURS_KEPT: usize = 24;
const RETRAIN_SECS: u64 = 600;

const N_TREES: usize = 100;
const SUBSAMPLE_SIZE: usize = 256;
const MAX_TREE_DEPTH: usize = 16; // ceil(log2(256)) + margin

// ---------------------------------------------------------------------------
// IsolationTree
// ---------------------------------------------------------------------------

enum ITree {
    Leaf {
        size: usize,
    },
    Node {
        feature: usize,
        threshold: f64,
        left: Box<ITree>,
        right: Box<ITree>,
    },
}

// Euler–Mascheroni written out: `f64::consts::EULER_GAMMA` is newer than the
// toolchains this builds on
#[allow(clippy::approx_constant)]
fn c(n: usize) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    let n = n as f64;
    2.0 * (n - 1.0).ln_1p() + 0.5772156649 - 2.0 * (n - 1.0) / n
}

impl ITree {
    fn build(data: &[Vec<f64>], depth: usize, rng: &mut impl Rng) -> Self {
        let n = data.len();
        if n <= 1 || depth >= MAX_TREE_DEPTH {
            return ITree::Leaf { size: n };
        }

        let dim = data[0].len();
        // Pick a random feature
        let feat = rng.gen_range(0..dim);

        let min = data.iter().map(|v| v[feat]).fold(f64::INFINITY, f64::min);
        let max = data
            .iter()
            .map(|v| v[feat])
            .fold(f64::NEG_INFINITY, f64::max);

        if (max - min).abs() < 1e-12 {
            return ITree::Leaf { size: n };
        }

        let threshold = rng.gen_range(min..max);

        let (left_data, right_data): (Vec<_>, Vec<_>) =
            data.iter().cloned().partition(|v| v[feat] <= threshold);

        ITree::Node {
            feature: feat,
            threshold,
            left: Box::new(ITree::build(&left_data, depth + 1, rng)),
            right: Box::new(ITree::build(&right_data, depth + 1, rng)),
        }
    }

    /// Returns the path length for point `x` in this tree.
    fn path_length(&self, x: &[f64], depth: f64) -> f64 {
        match self {
            ITree::Leaf { size } => depth + c(*size),
            ITree::Node {
                feature,
                threshold,
                left,
                right,
            } => {
                if x[*feature] <= *threshold {
                    left.path_length(x, depth + 1.0)
                } else {
                    right.path_length(x, depth + 1.0)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// IsolationForest
// ---------------------------------------------------------------------------

struct IsolationForestModel {
    trees: Vec<ITree>,
    n: usize, // subsample size used at train time
}

impl IsolationForestModel {
    fn build(data: &[Vec<f64>], rng: &mut impl Rng) -> Self {
        let n = SUBSAMPLE_SIZE.min(data.len());
        let trees = (0..N_TREES)
            .map(|_| {
                // Random subsample without replacement
                let mut idx: Vec<usize> = (0..data.len()).collect();
                // Fisher–Yates for first n elements
                for i in 0..n {
                    let j = rng.gen_range(i..data.len());
                    idx.swap(i, j);
                }
                let sample: Vec<Vec<f64>> = idx[..n].iter().map(|&i| data[i].clone()).collect();
                ITree::build(&sample, 0, rng)
            })
            .collect();
        Self { trees, n }
    }

    /// Anomaly score in [0, 1]. Higher = more anomalous.
    fn score(&self, x: &[f64]) -> f64 {
        let avg_path = self
            .trees
            .iter()
            .map(|t| t.path_length(x, 0.0))
            .sum::<f64>()
            / self.trees.len() as f64;
        let cn = c(self.n);
        if cn < 1e-12 {
            return 0.5;
        }
        2_f64.powf(-avg_path / cn)
    }
}

// ---------------------------------------------------------------------------
// Per-exporter model + 24 hourly reservoirs (task 17.10)
// ---------------------------------------------------------------------------

/// Uniform sample of one hour of minute vectors (Algorithm R)
struct HourReservoir {
    hour: u64,
    seen: u64,
    items: Vec<Vec<f64>>,
}

pub struct ExporterModel {
    forest: Option<IsolationForestModel>,
    hours: VecDeque<HourReservoir>,
    first_ts: Option<u64>,
    last_train_ts: u64,
    /// Score at or above which a minute counts as anomalous
    pub threshold: f64,
    /// Percentile the current threshold came from, for messages (e.g. 99.9)
    pub threshold_pct: f64,
    pub n_scored: u64,
}

impl ExporterModel {
    pub fn new() -> Self {
        Self {
            forest: None,
            hours: VecDeque::new(),
            first_ts: None,
            last_train_ts: 0,
            threshold: 1.0,
            threshold_pct: 100.0,
            n_scored: 0,
        }
    }

    /// Adds a minute vector observed at `ts` (unix seconds) to the baseline
    pub fn observe(&mut self, v: Vec<f64>, ts: u64, rng: &mut impl Rng) {
        self.first_ts.get_or_insert(ts);
        let hour = ts / 3_600;
        if self.hours.back().map(|h| h.hour) != Some(hour) {
            self.hours.push_back(HourReservoir {
                hour,
                seen: 0,
                items: Vec::new(),
            });
            while self.hours.len() > HOURS_KEPT {
                self.hours.pop_front();
            }
        }
        let r = self.hours.back_mut().expect("just pushed");
        r.seen += 1;
        if r.items.len() < HOUR_RESERVOIR {
            r.items.push(v);
        } else {
            let j = rng.gen_range(0..r.seen) as usize;
            if j < HOUR_RESERVOIR {
                r.items[j] = v;
            }
        }
    }

    pub fn samples_collected(&self) -> usize {
        self.hours.iter().map(|h| h.items.len()).sum()
    }

    fn warm(&self, now: u64) -> bool {
        self.samples_collected() >= WARMUP_SAMPLES
            && self
                .first_ts
                .is_some_and(|t| now.saturating_sub(t) >= WARMUP_SECS)
    }

    pub fn should_train(&self, now: u64) -> bool {
        self.warm(now)
            && (self.forest.is_none() || now.saturating_sub(self.last_train_ts) >= RETRAIN_SECS)
    }

    /// Trains on the whole baseline and sets the threshold to the
    /// `100 - alert_pct` percentile of the training scores. Call inside
    /// `block_in_place`.
    pub fn train(&mut self, now: u64, alert_pct: f64) {
        let data: Vec<Vec<f64>> = self
            .hours
            .iter()
            .flat_map(|h| h.items.iter().cloned())
            .collect();
        if data.is_empty() {
            return;
        }
        let mut rng = rand::thread_rng();
        let forest = IsolationForestModel::build(&data, &mut rng);
        let mut scores: Vec<f64> = data.iter().map(|v| forest.score(v)).collect();
        let pct = alert_pct.clamp(0.001, 50.0);
        self.threshold = percentile_threshold(&mut scores, pct).max(MIN_THRESHOLD);
        self.threshold_pct = 100.0 - pct;
        self.forest = Some(forest);
        self.last_train_ts = now;
    }

    /// Returns anomaly score [0,1] or `None` if not yet trained.
    pub fn score(&mut self, v: &[f64]) -> Option<f64> {
        self.n_scored += 1;
        self.forest.as_ref().map(|f| f.score(v))
    }

    pub fn is_ready(&self) -> bool {
        self.forest.is_some()
    }
}

/// Score that only the top `top_pct` percent of `scores` reach
fn percentile_threshold(scores: &mut [f64], top_pct: f64) -> f64 {
    scores.sort_unstable_by(f64::total_cmp);
    let n = scores.len();
    let idx = ((n as f64) * (1.0 - top_pct / 100.0)).ceil() as usize;
    scores[idx.saturating_sub(1).min(n - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_of(x: f64) -> Vec<f64> {
        vec![x; 9]
    }

    // AC-04 (task 17.10)
    #[test]
    fn reservoir_is_bounded_per_hour_and_in_hours() {
        let mut m = ExporterModel::new();
        let mut rng = rand::thread_rng();
        for i in 0..5_000u64 {
            m.observe(vec_of(i as f64), 0, &mut rng);
        }
        assert_eq!(m.samples_collected(), HOUR_RESERVOIR);
        for h in 1..=30u64 {
            m.observe(vec_of(0.0), h * 3_600, &mut rng);
        }
        assert_eq!(m.hours.len(), HOURS_KEPT);
    }

    #[test]
    fn needs_an_hour_and_enough_samples() {
        let mut m = ExporterModel::new();
        let mut rng = rand::thread_rng();
        // 3 hours × 2 000 vectors: enough samples, spread over the hour cap
        for i in 0..6_000u64 {
            m.observe(vec_of(i as f64), 1_000 + (i / 2_000) * 3_600, &mut rng);
        }
        assert!(m.samples_collected() >= WARMUP_SAMPLES);
        assert!(!m.should_train(1_000 + 1_800));
        assert!(m.should_train(1_000 + WARMUP_SECS));
    }

    // AC-03 (task 17.10)
    #[test]
    fn threshold_is_the_requested_percentile() {
        let mut scores: Vec<f64> = (1..=1000).map(|i| i as f64 / 1000.0).collect();
        assert_eq!(percentile_threshold(&mut scores, 0.1), 0.999);
        let mut scores: Vec<f64> = (1..=1000).map(|i| i as f64 / 1000.0).collect();
        assert_eq!(percentile_threshold(&mut scores, 10.0), 0.9);
    }

    #[test]
    fn trained_threshold_has_a_floor() {
        let mut m = ExporterModel::new();
        let mut rng = rand::thread_rng();
        for i in 0..3_000u64 {
            m.observe(vec_of((i % 7) as f64), 0, &mut rng);
        }
        m.train(10, 0.1);
        assert!(m.is_ready());
        assert!(m.threshold >= MIN_THRESHOLD);
        assert!((m.threshold_pct - 99.9).abs() < 1e-9);
    }
}
