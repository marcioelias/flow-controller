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

pub const WARMUP_SAMPLES:    usize = 5_000;
pub const RING_BUFFER_SIZE:  usize = 50_000;
pub const ANOMALY_THRESHOLD: f64   = 0.65;

const N_TREES:       usize = 100;
const SUBSAMPLE_SIZE: usize = 256;
const MAX_TREE_DEPTH: usize = 16; // ceil(log2(256)) + margin

// ---------------------------------------------------------------------------
// IsolationTree
// ---------------------------------------------------------------------------

enum ITree {
    Leaf { size: usize },
    Node {
        feature:   usize,
        threshold: f64,
        left:      Box<ITree>,
        right:     Box<ITree>,
    },
}

fn c(n: usize) -> f64 {
    if n <= 1 { return 0.0; }
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
        let max = data.iter().map(|v| v[feat]).fold(f64::NEG_INFINITY, f64::max);

        if (max - min).abs() < 1e-12 {
            return ITree::Leaf { size: n };
        }

        let threshold = rng.gen_range(min..max);

        let (left_data, right_data): (Vec<_>, Vec<_>) = data
            .iter()
            .cloned()
            .partition(|v| v[feat] <= threshold);

        ITree::Node {
            feature: feat,
            threshold,
            left:  Box::new(ITree::build(&left_data,  depth + 1, rng)),
            right: Box::new(ITree::build(&right_data, depth + 1, rng)),
        }
    }

    /// Returns the path length for point `x` in this tree.
    fn path_length(&self, x: &[f64], depth: f64) -> f64 {
        match self {
            ITree::Leaf { size } => depth + c(*size),
            ITree::Node { feature, threshold, left, right } => {
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
    n:     usize, // subsample size used at train time
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
        let avg_path = self.trees.iter().map(|t| t.path_length(x, 0.0)).sum::<f64>()
            / self.trees.len() as f64;
        let cn = c(self.n);
        if cn < 1e-12 { return 0.5; }
        2_f64.powf(-avg_path / cn)
    }
}

// ---------------------------------------------------------------------------
// Per-exporter model + ring buffer
// ---------------------------------------------------------------------------

pub struct ExporterModel {
    forest:       Option<IsolationForestModel>,
    pub buffer:   VecDeque<Vec<f64>>,
    pub n_scored: u64,
}

impl ExporterModel {
    pub fn new() -> Self {
        Self {
            forest:   None,
            buffer:   VecDeque::new(),
            n_scored: 0,
        }
    }

    /// Push a feature vector. Returns `true` when warm-up is done.
    pub fn push(&mut self, v: Vec<f64>) -> bool {
        if self.buffer.len() >= RING_BUFFER_SIZE {
            self.buffer.pop_front();
        }
        self.buffer.push_back(v);
        self.buffer.len() >= WARMUP_SAMPLES
    }

    /// Train (or retrain) from the ring buffer. Call inside `block_in_place`.
    pub fn train(&mut self) -> anyhow::Result<()> {
        let data: Vec<Vec<f64>> = self.buffer.iter().cloned().collect();
        let mut rng = rand::thread_rng();
        self.forest = Some(IsolationForestModel::build(&data, &mut rng));
        Ok(())
    }

    /// Returns anomaly score [0,1] or `None` if not yet trained.
    pub fn score(&mut self, v: &[f64]) -> Option<f64> {
        self.n_scored += 1;
        self.forest.as_ref().map(|f| f.score(v))
    }

    pub fn is_ready(&self) -> bool {
        self.forest.is_some()
    }

    pub fn samples_collected(&self) -> usize {
        self.buffer.len()
    }
}
