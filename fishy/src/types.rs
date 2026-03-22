use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use analysis::{SourceId, TemplateId};

// --- Input ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogCollection {
    pub sources: HashMap<SourceId, EventStream>,
    pub metadata: CollectionMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionMetadata {
    pub start_time: u64,
    pub end_time: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventStream {
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub template_id: TemplateId,
    pub timestamp: Option<u64>,
    pub params: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectConfig {
    pub mode: ComparisonMode,
    pub strategy: FusionStrategy,
    pub source_weights: Option<HashMap<SourceId, f32>>,
    pub significance_threshold: f32,
    pub duration_tolerance: f32,
}

impl Default for DetectConfig {
    fn default() -> Self {
        Self {
            mode: ComparisonMode::SingleOrigin,
            strategy: FusionStrategy::Adaptive,
            source_weights: None,
            significance_threshold: 0.5,
            duration_tolerance: 0.5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ComparisonMode {
    SingleOrigin,
    MultiOrigin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FusionStrategy {
    EvidenceConflict,
    DependencyShift,
    DistributionalFingerprint,
    SpectralFingerprint,
    Adaptive,
}

// --- Output ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnomalyReport {
    pub score: f64,
    /// m(Θ) from DS combination — how much evidence remains uncommitted.
    pub uncertainty: f64,
    pub verdict: String,
    pub source_scores: HashMap<SourceId, SourceReport>,
    pub pair_scores: Vec<PairReport>,
    pub missing_sources: MissingSourceReport,
    /// Degree of disagreement between analysis methods (DS conflict mass).
    pub meta_conflict: f64,
    /// Per-method breakdown from the adaptive fusion pass.
    pub methods: Vec<MethodDetail>,
}

/// Per-method detail from the adaptive fusion pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MethodDetail {
    pub name: String,
    pub applicable: bool,
    pub divergence: f64,
    pub entropy_delta: f64,
    pub baseline_entropy: f64,
    pub z_divergence: f64,
    pub z_entropy_delta: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceReport {
    pub divergence: f64,
    pub contribution: f64,
    pub top_events: Vec<(TemplateId, f64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairReport {
    pub source_a: SourceId,
    pub source_b: SourceId,
    pub dependency_shift: f64,
    pub baseline_correlation: f64,
    pub test_correlation: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissingSourceReport {
    pub baseline_only: Vec<SourceId>,
    pub test_only: Vec<SourceId>,
}

// --- Errors ---

#[derive(Clone, Debug)]
pub enum DetectError {
    TemporalMismatch { baseline_duration: u64, test_duration: u64 },
    EmptyCollection,
    NoOverlappingSources,
}

impl std::fmt::Display for DetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TemporalMismatch { baseline_duration, test_duration } => {
                write!(f, "temporal mismatch: baseline {baseline_duration}s, test {test_duration}s")
            }
            Self::EmptyCollection => write!(f, "empty collection"),
            Self::NoOverlappingSources => write!(f, "no overlapping sources in multi-origin mode"),
        }
    }
}

impl std::error::Error for DetectError {}
