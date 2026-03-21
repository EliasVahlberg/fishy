use crate::types::{MIMatrix, PowerSpectrum};

/// Shannon entropy of a probability distribution (nats).
pub fn shannon_entropy(distribution: &[f64]) -> f64 {
    distribution.iter().map(|&p| -p * (p + 1e-10).ln()).sum()
}

/// Spectral entropy: Shannon entropy of the normalized power spectrum.
pub fn spectral_entropy(spectrum: &PowerSpectrum) -> f64 {
    let total: f64 = spectrum.magnitudes.iter().sum();
    if total == 0.0 {
        return 0.0;
    }
    let normalized: Vec<f64> = spectrum.magnitudes.iter().map(|&m| m / total).collect();
    shannon_entropy(&normalized)
}

/// Entropy of the upper-triangle MI matrix values (normalized).
pub fn matrix_entropy(matrix: &MIMatrix) -> f64 {
    let n = matrix.sources.len();
    let vals: Vec<f64> = (0..n)
        .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
        .map(|(i, j)| matrix.values[i][j])
        .collect();
    let total: f64 = vals.iter().sum();
    if total == 0.0 {
        return 0.0;
    }
    let normalized: Vec<f64> = vals.iter().map(|&v| v / total).collect();
    shannon_entropy(&normalized)
}
