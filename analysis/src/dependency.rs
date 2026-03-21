use crate::types::{MIMatrix, SourceId, TemplateId};
use std::collections::HashMap;

/// Compute mutual information matrix between all source pairs.
///
/// MI(X;Y) = H(X) + H(Y) - H(X,Y), estimated from empirical joint counts.
pub fn mutual_information_matrix(collection: &[(SourceId, &[TemplateId])]) -> MIMatrix {
    let n = collection.len();
    let mut values = vec![vec![0.0f64; n]; n];

    for i in 0..n {
        for j in i..n {
            let mi = if i == j {
                marginal_entropy(collection[i].1)
            } else {
                pairwise_mi(collection[i].1, collection[j].1)
            };
            values[i][j] = mi;
            values[j][i] = mi;
        }
    }

    MIMatrix { sources: collection.iter().map(|(id, _)| *id).collect(), values }
}

/// Normalized Frobenius norm of MI matrix difference: ||A-B||_F / max(||A||_F, ||B||_F).
pub fn mi_matrix_divergence(baseline: &MIMatrix, test: &MIMatrix) -> f64 {
    // Align by shared sources in baseline order.
    let shared: Vec<_> = baseline
        .sources
        .iter()
        .filter(|id| test.sources.contains(id))
        .copied()
        .collect();
    if shared.is_empty() {
        return 0.0;
    }

    let b_idx: HashMap<SourceId, usize> =
        baseline.sources.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let t_idx: HashMap<SourceId, usize> =
        test.sources.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    let (mut diff_sq, mut b_sq, mut t_sq) = (0.0f64, 0.0f64, 0.0f64);
    for &a in &shared {
        for &b in &shared {
            let bv = baseline.values[b_idx[&a]][b_idx[&b]];
            let tv = test.values[t_idx[&a]][t_idx[&b]];
            diff_sq += (bv - tv).powi(2);
            b_sq += bv.powi(2);
            t_sq += tv.powi(2);
        }
    }

    let denom = b_sq.sqrt().max(t_sq.sqrt());
    if denom < 1e-10 { 0.0 } else { (diff_sq.sqrt() / denom).clamp(0.0, 1.0) }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn marginal_entropy(events: &[TemplateId]) -> f64 {
    let counts = count(events);
    let total = events.len() as f64;
    counts.values().map(|&c| {
        let p = c as f64 / total;
        -p * (p + 1e-10).ln()
    }).sum()
}

fn pairwise_mi(xs: &[TemplateId], ys: &[TemplateId]) -> f64 {
    if xs.is_empty() || ys.is_empty() {
        return 0.0;
    }
    // Pair events by position (zip); treat as joint observations.
    let pairs: Vec<_> = xs.iter().zip(ys.iter()).collect();
    let n = pairs.len() as f64;

    let mut joint: HashMap<(TemplateId, TemplateId), u64> = HashMap::new();
    for (&x, &y) in &pairs {
        *joint.entry((x, y)).or_insert(0) += 1;
    }
    let cx = count(xs);
    let cy = count(ys);

    joint.iter().map(|((x, y), &c)| {
        let p_xy = c as f64 / n;
        let p_x = cx[x] as f64 / xs.len() as f64;
        let p_y = cy[y] as f64 / ys.len() as f64;
        p_xy * (p_xy / (p_x * p_y + 1e-10)).ln()
    }).sum::<f64>().max(0.0)
}

fn count(events: &[TemplateId]) -> HashMap<TemplateId, u64> {
    let mut m = HashMap::new();
    for &e in events {
        *m.entry(e).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tid(n: u32) -> TemplateId { TemplateId(n) }
    fn sid(n: u32) -> SourceId { SourceId(n) }

    #[test]
    fn identical_matrices_zero_divergence() {
        let s0 = vec![tid(1), tid(2), tid(1), tid(2)];
        let s1 = vec![tid(3), tid(3), tid(4), tid(3)];
        let col = vec![(sid(0), s0.as_slice()), (sid(1), s1.as_slice())];
        let m = mutual_information_matrix(&col);
        assert_eq!(mi_matrix_divergence(&m, &m), 0.0);
    }

    #[test]
    fn mi_diagonal_is_entropy() {
        // A single source with uniform distribution has positive entropy.
        let s = vec![tid(1), tid(2), tid(3), tid(4)];
        let col = vec![(sid(0), s.as_slice())];
        let m = mutual_information_matrix(&col);
        assert!(m.values[0][0] > 0.0);
    }
}
