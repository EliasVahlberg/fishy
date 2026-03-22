use crate::types::{MIMatrix, SourceId, TemplateId};
use std::collections::HashMap;

/// Compute mutual information matrix between all source pairs.
///
/// MI is estimated from temporal co-occurrence: both sources are binned into
/// fixed-width time windows, and MI is computed from the joint distribution of
/// (template_in_source_A, template_in_source_B) within the same bin.
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

/// MI between two sources estimated from co-occurrence in shared time bins.
///
/// Events are `(TemplateId, timestamp)` encoded as interleaved pairs in the
/// slice: the public API passes `&[TemplateId]` without timestamps, so we fall
/// back to uniform binning by position when no timing information is available.
/// The caller in `fishy` passes timed events via the separate
/// `mutual_information_matrix_timed` entry point.
fn pairwise_mi(xs: &[TemplateId], ys: &[TemplateId]) -> f64 {
    if xs.is_empty() || ys.is_empty() {
        return 0.0;
    }
    // Bin both sequences into equal-sized buckets (positional bins).
    // This is the fallback for callers that don't have timestamps.
    // fishy uses mutual_information_matrix_timed instead.
    let n_bins = (xs.len().max(ys.len()) as f64).sqrt().ceil() as usize + 1;
    let x_bin: Vec<(TemplateId, usize)> = xs.iter().enumerate()
        .map(|(i, &t)| (t, i * n_bins / xs.len()))
        .collect();
    let y_bin: Vec<(TemplateId, usize)> = ys.iter().enumerate()
        .map(|(i, &t)| (t, i * n_bins / ys.len()))
        .collect();
    mi_from_bins(&x_bin, &y_bin)
}

/// MI between two timed event streams, binned by wall-clock time.
pub fn mutual_information_matrix_timed(
    collection: &[(SourceId, &[(TemplateId, u64)])],
    bin_width: u64,
) -> MIMatrix {
    let n = collection.len();
    let mut values = vec![vec![0.0f64; n]; n];

    for i in 0..n {
        for j in i..n {
            let mi = if i == j {
                let ids: Vec<TemplateId> = collection[i].1.iter().map(|(t, _)| *t).collect();
                marginal_entropy(&ids)
            } else {
                let xs: Vec<(TemplateId, usize)> = collection[i].1.iter()
                    .map(|&(t, ts)| (t, (ts / bin_width.max(1)) as usize))
                    .collect();
                let ys: Vec<(TemplateId, usize)> = collection[j].1.iter()
                    .map(|&(t, ts)| (t, (ts / bin_width.max(1)) as usize))
                    .collect();
                mi_from_bins(&xs, &ys)
            };
            values[i][j] = mi;
            values[j][i] = mi;
        }
    }

    MIMatrix { sources: collection.iter().map(|(id, _)| *id).collect(), values }
}

/// Compute MI from two sequences of (template, bin_index) pairs.
fn mi_from_bins(xs: &[(TemplateId, usize)], ys: &[(TemplateId, usize)]) -> f64 {
    if xs.is_empty() || ys.is_empty() {
        return 0.0;
    }
    // Build per-bin template distributions for each source.
    let mut x_bins: HashMap<usize, HashMap<TemplateId, u64>> = HashMap::new();
    for &(t, b) in xs {
        *x_bins.entry(b).or_default().entry(t).or_insert(0) += 1;
    }
    let mut y_bins: HashMap<usize, HashMap<TemplateId, u64>> = HashMap::new();
    for &(t, b) in ys {
        *y_bins.entry(b).or_default().entry(t).or_insert(0) += 1;
    }

    // Joint distribution over (x_template, y_template) for bins present in both.
    let mut joint: HashMap<(TemplateId, TemplateId), u64> = HashMap::new();
    let mut n_joint = 0u64;
    for (bin, xm) in &x_bins {
        if let Some(ym) = y_bins.get(bin) {
            for (&xt, &xc) in xm {
                for (&yt, &yc) in ym {
                    *joint.entry((xt, yt)).or_insert(0) += xc * yc;
                    n_joint += xc * yc;
                }
            }
        }
    }
    if n_joint == 0 {
        return 0.0;
    }

    let n = n_joint as f64;
    let cx: HashMap<TemplateId, u64> = xs.iter().fold(HashMap::new(), |mut m, &(t, _)| {
        *m.entry(t).or_insert(0) += 1; m
    });
    let cy: HashMap<TemplateId, u64> = ys.iter().fold(HashMap::new(), |mut m, &(t, _)| {
        *m.entry(t).or_insert(1) += 1; m
    });
    let nx = xs.len() as f64;
    let ny = ys.len() as f64;

    joint.iter().map(|((x, y), &c)| {
        let p_xy = c as f64 / n;
        let p_x = cx.get(x).copied().unwrap_or(0) as f64 / nx;
        let p_y = cy.get(y).copied().unwrap_or(0) as f64 / ny;
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
