use crate::types::{EigenSpectrum, TemplateId};
use nalgebra::DMatrix;
use std::collections::HashMap;

/// Laplacian eigenvalue spectrum of an event co-occurrence graph.
///
/// Nodes = unique template IDs. Edge weight = number of times two templates
/// appear within `window` time units of each other. Eigenvalues computed via
/// the QR algorithm on the symmetric Laplacian.
/// Maximum number of distinct templates kept for the co-occurrence graph.
/// Keeps the Laplacian small enough for Jacobi eigenvalue decomposition.
const MAX_CO_NODES: usize = 128;

pub fn co_occurrence_spectrum(events: &[(TemplateId, u64)], window: u64) -> EigenSpectrum {
    if events.len() < 2 {
        return EigenSpectrum { eigenvalues: vec![] };
    }

    // Keep only the top-K most frequent templates.
    let keep: std::collections::HashSet<TemplateId> = {
        let mut freq: HashMap<TemplateId, usize> = HashMap::new();
        for &(t, _) in events { *freq.entry(t).or_insert(0) += 1; }
        let mut pairs: Vec<_> = freq.into_iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        pairs.truncate(MAX_CO_NODES);
        pairs.into_iter().map(|(t, _)| t).collect()
    };

    let filtered: Vec<(TemplateId, u64)> = events.iter()
        .filter(|(t, _)| keep.contains(t))
        .copied()
        .collect();

    if filtered.len() < 2 {
        return EigenSpectrum { eigenvalues: vec![] };
    }

    // Build adjacency weights.
    let mut adj: HashMap<(TemplateId, TemplateId), f64> = HashMap::new();
    let mut sorted = filtered;
    sorted.sort_by_key(|(_, t)| *t);

    let mut left = 0usize;
    for right in 0..sorted.len() {
        while sorted[right].1.saturating_sub(sorted[left].1) > window {
            left += 1;
        }
        for k in left..right {
            let a = sorted[k].0;
            let b = sorted[right].0;
            if a != b {
                let key = if a.0 < b.0 { (a, b) } else { (b, a) };
                *adj.entry(key).or_insert(0.0) += 1.0;
            }
        }
    }

    // Index nodes.
    let nodes: Vec<TemplateId> = {
        let mut v: Vec<_> = keep.into_iter().collect();
        v.sort_by_key(|t| t.0);
        v
    };
    let n = nodes.len();
    let idx: HashMap<TemplateId, usize> =
        nodes.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    // Build Laplacian L = D - A as a nalgebra DMatrix.
    let mut lap = DMatrix::<f64>::zeros(n, n);
    for (&(a, b), &w) in &adj {
        let i = idx[&a];
        let j = idx[&b];
        lap[(i, j)] -= w;
        lap[(j, i)] -= w;
        lap[(i, i)] += w;
        lap[(j, j)] += w;
    }

    let decomp = lap.symmetric_eigen();
    let mut eigenvalues: Vec<f64> = decomp.eigenvalues.iter().copied().collect();
    eigenvalues.sort_by(|a, b| a.partial_cmp(b).unwrap());
    EigenSpectrum { eigenvalues }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid(n: u32) -> TemplateId { TemplateId(n) }

    #[test]
    fn single_connected_component_has_one_zero_eigenvalue() {
        // A fully connected graph has exactly one zero eigenvalue.
        let events = vec![
            (tid(1), 0), (tid(2), 5), (tid(3), 8),
            (tid(1), 20), (tid(2), 22),
        ];
        let spec = co_occurrence_spectrum(&events, 15);
        let zeros = spec.eigenvalues.iter().filter(|&&v| v.abs() < 1e-6).count();
        assert_eq!(zeros, 1, "eigenvalues: {:?}", spec.eigenvalues);
    }

    #[test]
    fn two_components_have_two_zero_eigenvalues() {
        // Events far apart in time → two disconnected components.
        let events = vec![
            (tid(1), 0), (tid(2), 5),
            (tid(3), 10000), (tid(4), 10005),
        ];
        let spec = co_occurrence_spectrum(&events, 10);
        let zeros = spec.eigenvalues.iter().filter(|&&v| v.abs() < 1e-6).count();
        assert_eq!(zeros, 2, "eigenvalues: {:?}", spec.eigenvalues);
    }
}
