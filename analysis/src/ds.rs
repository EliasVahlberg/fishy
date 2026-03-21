use crate::types::BPA;

/// Dempster's rule of combination for two BPAs over {normal, anomalous}.
pub fn ds_combine(a: &BPA, b: &BPA) -> BPA {
    let conflict = a.normal * b.anomalous + a.anomalous * b.normal;
    let denom = 1.0 - conflict;
    if denom < 1e-10 {
        return BPA { normal: 0.0, anomalous: 0.0, uncertain: 1.0 };
    }
    let normal = (a.normal * b.normal + a.normal * b.uncertain + a.uncertain * b.normal) / denom;
    let anomalous =
        (a.anomalous * b.anomalous + a.anomalous * b.uncertain + a.uncertain * b.anomalous)
            / denom;
    let uncertain = (a.uncertain * b.uncertain) / denom;
    BPA {
        normal: normal.clamp(0.0, 1.0),
        anomalous: anomalous.clamp(0.0, 1.0),
        uncertain: uncertain.clamp(0.0, 1.0),
    }
}

/// Combine multiple BPAs via iterated Dempster's rule.
pub fn ds_combine_many(bpas: &[BPA]) -> BPA {
    match bpas {
        [] => BPA { normal: 0.0, anomalous: 0.0, uncertain: 1.0 },
        [single] => single.clone(),
        [first, rest @ ..] => rest.iter().fold(first.clone(), |acc, b| ds_combine(&acc, b)),
    }
}

/// Conflict mass between two BPAs.
pub fn ds_conflict(a: &BPA, b: &BPA) -> f64 {
    a.normal * b.anomalous + a.anomalous * b.normal
}
