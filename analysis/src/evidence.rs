use crate::types::{BpaMapping, BPA};

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

/// Construct a BPA from a z-score using the given mapping.
///
/// Positive z → anomalous mass, negative z → normal mass.
/// |z| determines commitment (how much mass goes to a hypothesis vs uncertain).
/// Near-zero z → mostly uncertain.
pub fn bpa_from_zscore(z: f64, mapping: &BpaMapping) -> BPA {
    let commitment = match mapping {
        BpaMapping::Sigmoid { midpoint } => 1.0 / (1.0 + (-(z.abs() - midpoint)).exp()),
        BpaMapping::Proportional { z_max } => (z.abs() / z_max).clamp(0.0, 1.0),
    };
    if z >= 0.0 {
        BPA { anomalous: commitment, normal: 0.0, uncertain: 1.0 - commitment }
    } else {
        BPA { normal: commitment, anomalous: 0.0, uncertain: 1.0 - commitment }
    }
}
