use crate::types::EventDistribution;

/// Jensen-Shannon divergence between two event distributions. Returns [0, 1].
pub fn distributional_divergence(baseline: &EventDistribution, test: &EventDistribution) -> f64 {
    // Collect the union of all template IDs.
    let keys: std::collections::HashSet<_> =
        baseline.counts.keys().chain(test.counts.keys()).collect();

    let b_total = baseline.total.max(1) as f64;
    let t_total = test.total.max(1) as f64;

    let (kl_bm, kl_tm) = keys.iter().fold((0.0, 0.0), |(kb, kt), &id| {
        let p = baseline.counts.get(id).copied().unwrap_or(0) as f64 / b_total;
        let q = test.counts.get(id).copied().unwrap_or(0) as f64 / t_total;
        let m = 0.5 * (p + q);
        let kb = kb + if p > 0.0 { p * (p / (m + 1e-10)).ln() } else { 0.0 };
        let kt = kt + if q > 0.0 { q * (q / (m + 1e-10)).ln() } else { 0.0 };
        (kb, kt)
    });

    // JSD in nats; divide by ln(2) to normalise to [0, 1].
    (0.5 * (kl_bm + kl_tm) / std::f64::consts::LN_2).clamp(0.0, 1.0)
}
