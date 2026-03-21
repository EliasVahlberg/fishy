use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Newtype for source identifiers.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceId(pub u32);

/// Newtype for event template identifiers.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemplateId(pub u32);

/// Probability distribution over template IDs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDistribution {
    pub counts: HashMap<TemplateId, u64>,
    pub total: u64,
}

/// Mutual information between source pairs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MIMatrix {
    pub sources: Vec<SourceId>,
    /// Symmetric matrix, sources.len() × sources.len().
    pub values: Vec<Vec<f64>>,
}

/// FFT power spectrum of an event rate time series.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowerSpectrum {
    pub frequencies: Vec<f64>,
    pub magnitudes: Vec<f64>,
}

/// Wavelet decomposition coefficients at multiple resolution levels.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaveletCoefficients {
    /// Coarse to fine.
    pub levels: Vec<Vec<f64>>,
}

/// Laplacian eigenvalue spectrum of a co-occurrence graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EigenSpectrum {
    /// Sorted ascending.
    pub eigenvalues: Vec<f64>,
}

/// Dempster-Shafer basic probability assignment over {normal, anomalous}.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BPA {
    pub normal: f64,
    pub anomalous: f64,
    /// 1.0 - normal - anomalous.
    pub uncertain: f64,
}
