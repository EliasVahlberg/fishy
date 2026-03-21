use crate::types::BPA;

/// Construct a BPA from a divergence score and confidence.
///
/// - `divergence` ∈ [0, 1]: how anomalous the signal is
/// - `confidence` ∈ [0, 1]: how certain the method is
pub fn evidence_bpa(divergence: f64, confidence: f64) -> BPA {
    let d = divergence.clamp(0.0, 1.0);
    let c = confidence.clamp(0.0, 1.0);
    BPA {
        anomalous: d * c,
        normal: (1.0 - d) * c,
        uncertain: 1.0 - c,
    }
}
