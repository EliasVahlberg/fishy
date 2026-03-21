use crate::types::{EigenSpectrum, TemplateId};
use std::collections::HashMap;

/// Laplacian eigenvalue spectrum of an event co-occurrence graph.
///
/// Nodes = unique template IDs. Edge weight = number of times two templates
/// appear within `window` time units of each other. Eigenvalues computed via
/// the QR algorithm on the symmetric Laplacian.
pub fn co_occurrence_spectrum(events: &[(TemplateId, u64)], window: u64) -> EigenSpectrum {
    if events.len() < 2 {
        return EigenSpectrum { eigenvalues: vec![] };
    }

    // Build adjacency weights.
    let mut adj: HashMap<(TemplateId, TemplateId), f64> = HashMap::new();
    let mut sorted = events.to_vec();
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
        let mut s: std::collections::HashSet<TemplateId> =
            events.iter().map(|(t, _)| *t).collect();
        let mut v: Vec<_> = s.drain().collect();
        v.sort_by_key(|t| t.0);
        v
    };
    let n = nodes.len();
    let idx: HashMap<TemplateId, usize> =
        nodes.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    // Build Laplacian L = D - A.
    let mut lap = vec![vec![0.0f64; n]; n];
    for (&(a, b), &w) in &adj {
        let i = idx[&a];
        let j = idx[&b];
        lap[i][j] -= w;
        lap[j][i] -= w;
        lap[i][i] += w;
        lap[j][j] += w;
    }

    let mut eigenvalues = symmetric_eigenvalues(lap);
    eigenvalues.sort_by(|a, b| a.partial_cmp(b).unwrap());
    EigenSpectrum { eigenvalues }
}

// ---------------------------------------------------------------------------
// Symmetric eigenvalue solver via Jacobi iterations
// ---------------------------------------------------------------------------

fn symmetric_eigenvalues(mut a: Vec<Vec<f64>>) -> Vec<f64> {
    let n = a.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![a[0][0]];
    }

    // Jacobi sweeps until off-diagonal norm is negligible.
    for _ in 0..100 * n * n {
        // Find largest off-diagonal element.
        let (mut p, mut q, mut max_val) = (0, 1, 0.0f64);
        for i in 0..n {
            for j in i + 1..n {
                if a[i][j].abs() > max_val {
                    max_val = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        if max_val < 1e-10 {
            break;
        }

        // Compute Jacobi rotation angle.
        let theta = 0.5 * (a[q][q] - a[p][p]) / (a[p][q] + 1e-30);
        let t = theta.signum() / (theta.abs() + (1.0 + theta * theta).sqrt());
        let c = 1.0 / (1.0 + t * t).sqrt();
        let s = t * c;

        // Apply rotation.
        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
        for r in 0..n {
            if r != p && r != q {
                let arp = a[r][p];
                let arq = a[r][q];
                a[r][p] = c * arp - s * arq;
                a[p][r] = a[r][p];
                a[r][q] = s * arp + c * arq;
                a[q][r] = a[r][q];
            }
        }
    }

    (0..n).map(|i| a[i][i]).collect()
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
