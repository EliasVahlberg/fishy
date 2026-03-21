//! Multi-source information-fusion anomaly detection through collection comparison.
//!
//! ```ignore
//! let report = fishy::detect(&baseline, &test, &DetectConfig::default())?;
//! println!("fishy score: {:.2}", report.score);
//! ```

mod extract;
mod loader;
mod types;

#[cfg(feature = "parallel")]
use rayon::join;

pub use loader::load_collection;
pub use types::*;

use analysis::{
    distributional_divergence, evidence_bpa, matrix_entropy, mi_matrix_divergence,
    shannon_entropy, spectral_divergence, spectral_entropy, BPA,
};
use extract::{extract, to_distribution};
use std::collections::HashMap;

pub fn detect(
    baseline: &LogCollection,
    test: &LogCollection,
    config: &DetectConfig,
) -> Result<AnomalyReport, DetectError> {
    if baseline.sources.is_empty() || test.sources.is_empty() {
        return Err(DetectError::EmptyCollection);
    }

    let b_dur = baseline.metadata.end_time.saturating_sub(baseline.metadata.start_time);
    let t_dur = test.metadata.end_time.saturating_sub(test.metadata.start_time);
    let tol = config.duration_tolerance as f64;
    if b_dur > 0 && t_dur > 0 {
        let ratio = b_dur as f64 / t_dur as f64;
        if ratio < (1.0 - tol) || ratio > (1.0 + tol) {
            return Err(DetectError::TemporalMismatch {
                baseline_duration: b_dur,
                test_duration: t_dur,
            });
        }
    }

    let (pairs, missing) = source_pairs(baseline, test, &config.mode)?;

    match config.strategy {
        FusionStrategy::Adaptive => adaptive(baseline, test, &pairs, &missing, config),
        FusionStrategy::DistributionalFingerprint => {
            distributional_only(baseline, test, &pairs, &missing, config)
        }
        _ => distributional_only(baseline, test, &pairs, &missing, config),
    }
}

// ---------------------------------------------------------------------------
// Adaptive fusion
// ---------------------------------------------------------------------------

fn adaptive(
    baseline: &LogCollection,
    test: &LogCollection,
    pairs: &[SourceId],
    missing: &MissingSourceReport,
    config: &DetectConfig,
) -> Result<AnomalyReport, DetectError> {
    let b_rep = extract(baseline);
    let t_rep = extract(test);

    // --- Per-source distributional divergences ---
    let mut source_scores: HashMap<SourceId, SourceReport> = HashMap::new();
    for &id in &missing.baseline_only {
        source_scores.insert(
            id,
            SourceReport { divergence: 1.0, contribution: 1.0, top_events: vec![] },
        );
    }
    for &id in pairs {
        let div = distributional_divergence(&b_rep.distributions[&id], &t_rep.distributions[&id]);
        source_scores.insert(
            id,
            SourceReport { divergence: div, contribution: 0.0, top_events: vec![] },
        );
    }

    // --- Method divergences (max across sources — any anomalous source flags the collection) ---
    // 1. Distributional: max JSD across paired sources + 1.0 per missing source.
    let dist_div = {
        let missing_max: f64 = if missing.baseline_only.is_empty() { 0.0 } else { 1.0 };
        let paired_max: f64 = pairs
            .iter()
            .map(|id| source_scores[id].divergence)
            .fold(0.0f64, f64::max);
        missing_max.max(paired_max)
    };

    // 2. Dependency shift: MI matrix divergence — skip if <2 paired sources.
    let dep_div = if pairs.len() >= 2 {
        match (&b_rep.mi_matrix, &t_rep.mi_matrix) {
            (Some(bm), Some(tm)) => mi_matrix_divergence(bm, tm),
            _ => 0.0,
        }
    } else {
        0.0
    };

    // Minimum events required for frequency-domain methods to be meaningful.
    const MIN_SPECTRAL_EVENTS: usize = 32;

    // 3. Spectral: max spectral divergence across paired sources.
    let spec_div = pairs
        .iter()
        .filter_map(|id| {
            let stream = baseline.sources.get(id)?;
            if stream.events.len() < MIN_SPECTRAL_EVENTS {
                return None;
            }
            let bs = b_rep.spectra.get(id)?;
            let ts = t_rep.spectra.get(id)?;
            if bs.magnitudes.is_empty() || ts.magnitudes.is_empty() {
                None
            } else {
                Some(spectral_divergence(bs, ts))
            }
        })
        .fold(0.0f64, f64::max);

    // 4. Co-occurrence: max eigenspectrum divergence across paired sources.
    let co_div = pairs
        .iter()
        .filter_map(|id| {
            let stream = baseline.sources.get(id)?;
            if stream.events.len() < MIN_SPECTRAL_EVENTS {
                return None;
            }
            Some(eigen_divergence(b_rep.eigen.get(id)?, t_rep.eigen.get(id)?))
        })
        .fold(0.0f64, f64::max);

    // 5. Evidence conflict: pairwise DS conflict between per-source BPAs.
    let conflict_div = {
        let bpas: Vec<BPA> = pairs
            .iter()
            .map(|id| evidence_bpa(source_scores[id].divergence, 1.0))
            .collect();
        if bpas.len() < 2 {
            0.0
        } else {
            let mut max_conflict = 0.0f64;
            for i in 0..bpas.len() {
                for j in i + 1..bpas.len() {
                    max_conflict = max_conflict.max(analysis::ds_conflict(&bpas[i], &bpas[j]));
                }
            }
            max_conflict
        }
    };

    // --- Perceived entropy per method (from baseline) ---
    let dist_entropy = {
        let all_counts: Vec<f64> = b_rep
            .distributions
            .values()
            .flat_map(|d| {
                let total = d.total.max(1) as f64;
                d.counts.values().map(move |&c| c as f64 / total)
            })
            .collect();
        shannon_entropy(&all_counts)
    };

    let dep_entropy = b_rep.mi_matrix.as_ref().map(|m| matrix_entropy(m)).unwrap_or(0.0);

    let spec_entropy = {
        let vals: Vec<f64> = b_rep.spectra.values().map(|s| spectral_entropy(s)).collect();
        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
    };

    let co_entropy = {
        let vals: Vec<f64> = b_rep
            .eigen
            .values()
            .map(|e| {
                let total: f64 = e.eigenvalues.iter().sum();
                if total < 1e-10 {
                    return 0.0;
                }
                let norm: Vec<f64> = e.eigenvalues.iter().map(|&v| v / total).collect();
                shannon_entropy(&norm)
            })
            .collect();
        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
    };

    // Conflict entropy: entropy of the per-source divergence distribution.
    let conflict_entropy = {
        let divs: Vec<f64> = pairs.iter().map(|id| source_scores[id].divergence).collect();
        let total: f64 = divs.iter().sum();
        if total < 1e-10 {
            0.0
        } else {
            let norm: Vec<f64> = divs.iter().map(|&d| d / total).collect();
            shannon_entropy(&norm)
        }
    };

    // --- Baseline stability (sub-sample variance) — most expensive step ---
    #[cfg(feature = "parallel")]
    let b_stability = {
        let (dist_var, (dep_var, (spec_var, co_var))) = join(
            || baseline_stability_dist(baseline, pairs),
            || {
                join(
                    || baseline_stability_dep(baseline, pairs),
                    || join(
                        || baseline_stability_spec(baseline, pairs),
                        || baseline_stability_co(baseline, pairs),
                    ),
                )
            },
        );
        Stability { dist: dist_var, dep: dep_var, spec: spec_var, co: co_var, conflict: 0.0 }
    };
    #[cfg(not(feature = "parallel"))]
    let b_stability = baseline_stability(baseline, pairs);

    // --- Weights: entropy × 1/(1+variance) ---
    let methods: &[(&str, f64, f64, f64)] = &[
        ("dist", dist_div, dist_entropy, b_stability.dist),
        ("dep", dep_div, dep_entropy, b_stability.dep),
        ("spec", spec_div, spec_entropy, b_stability.spec),
        ("co", co_div, co_entropy, b_stability.co),
        ("conflict", conflict_div, conflict_entropy, b_stability.conflict),
    ];

    let weights: Vec<f64> = methods
        .iter()
        .map(|(_, _, entropy, variance)| entropy / (1.0 + variance))
        .collect();
    let weight_sum: f64 = weights.iter().sum();

    // --- DS meta-combination (for meta_conflict signal only) ---
    let meta_bpas: Vec<BPA> = methods
        .iter()
        .zip(weights.iter())
        .map(|((_, div, _, _), &w)| {
            let confidence = if weight_sum > 1e-10 { (w / weight_sum).clamp(0.0, 1.0) } else { 0.2 };
            evidence_bpa(*div, confidence)
        })
        .collect();

    let meta_conflict: f64 = {
        let mut total = 0.0f64;
        let mut count = 0usize;
        for i in 0..meta_bpas.len() {
            for j in i + 1..meta_bpas.len() {
                total += analysis::ds_conflict(&meta_bpas[i], &meta_bpas[j]);
                count += 1;
            }
        }
        if count > 0 { total / count as f64 } else { 0.0 }
    };

    // Score = weighted mean of method divergences.
    // DS combination is used only for meta_conflict (inter-method disagreement signal).
    let score = if weight_sum > 1e-10 {
        methods.iter().zip(weights.iter()).map(|((_, div, _, _), &w)| div * w).sum::<f64>()
            / weight_sum
    } else {
        methods.iter().map(|(_, div, _, _)| *div).sum::<f64>() / methods.len() as f64
    }
    .clamp(0.0, 1.0);

    // Update source contribution from final score.
    for report in source_scores.values_mut() {
        report.contribution = report.divergence * score;
    }

    let verdict = verdict_string(score, config.significance_threshold as f64);

    Ok(AnomalyReport {
        score,
        verdict,
        source_scores,
        pair_scores: vec![],
        missing_sources: missing.clone(),
        meta_conflict,
    })
}

// ---------------------------------------------------------------------------
// Distributional-only path (non-adaptive strategies)
// ---------------------------------------------------------------------------

fn distributional_only(
    baseline: &LogCollection,
    test: &LogCollection,
    pairs: &[SourceId],
    missing: &MissingSourceReport,
    config: &DetectConfig,
) -> Result<AnomalyReport, DetectError> {
    let mut source_scores: HashMap<SourceId, SourceReport> = HashMap::new();
    let mut total_div = 0.0f64;
    let n = (pairs.len() + missing.baseline_only.len()).max(1);

    for &id in &missing.baseline_only {
        total_div += 1.0;
        source_scores.insert(
            id,
            SourceReport { divergence: 1.0, contribution: 1.0, top_events: vec![] },
        );
    }
    for &id in pairs {
        let b_dist = to_distribution(&baseline.sources[&id]);
        let t_dist = to_distribution(&test.sources[&id]);
        let div = distributional_divergence(&b_dist, &t_dist);
        total_div += div;
        source_scores.insert(
            id,
            SourceReport { divergence: div, contribution: div, top_events: vec![] },
        );
    }

    let score = (total_div / n as f64).clamp(0.0, 1.0);
    let verdict = verdict_string(score, config.significance_threshold as f64);
    Ok(AnomalyReport {
        score,
        verdict,
        source_scores,
        pair_scores: vec![],
        missing_sources: missing.clone(),
        meta_conflict: 0.0,
    })
}

// ---------------------------------------------------------------------------
// Baseline stability estimation
// ---------------------------------------------------------------------------

struct Stability {
    dist: f64,
    dep: f64,
    spec: f64,
    co: f64,
    conflict: f64,
}

/// Split baseline into two halves, run each method on both halves, return variance proxy.
#[cfg(not(feature = "parallel"))]
fn baseline_stability(baseline: &LogCollection, pairs: &[SourceId]) -> Stability {
    if pairs.is_empty() {
        return Stability { dist: 0.0, dep: 0.0, spec: 0.0, co: 0.0, conflict: 0.0 };
    }
    let (half_a, half_b) = split_collection(baseline, pairs);
    let ra = extract(&half_a);
    let rb = extract(&half_b);
    Stability {
        dist: baseline_stability_dist_reps(&ra, &rb, pairs),
        dep: baseline_stability_dep_reps(&ra, &rb),
        spec: baseline_stability_spec_reps(&ra, &rb, pairs),
        co: baseline_stability_co_reps(&ra, &rb, pairs),
        conflict: 0.0,
    }
}

#[cfg(feature = "parallel")]
fn baseline_stability_dist(baseline: &LogCollection, pairs: &[SourceId]) -> f64 {
    if pairs.is_empty() { return 0.0; }
    let (a, b) = split_collection(baseline, pairs);
    baseline_stability_dist_reps(&extract(&a), &extract(&b), pairs)
}
#[cfg(feature = "parallel")]
fn baseline_stability_dep(baseline: &LogCollection, pairs: &[SourceId]) -> f64 {
    if pairs.is_empty() { return 0.0; }
    let (a, b) = split_collection(baseline, pairs);
    baseline_stability_dep_reps(&extract(&a), &extract(&b))
}
#[cfg(feature = "parallel")]
fn baseline_stability_spec(baseline: &LogCollection, pairs: &[SourceId]) -> f64 {
    if pairs.is_empty() { return 0.0; }
    let (a, b) = split_collection(baseline, pairs);
    baseline_stability_spec_reps(&extract(&a), &extract(&b), pairs)
}
#[cfg(feature = "parallel")]
fn baseline_stability_co(baseline: &LogCollection, pairs: &[SourceId]) -> f64 {
    if pairs.is_empty() { return 0.0; }
    let (a, b) = split_collection(baseline, pairs);
    baseline_stability_co_reps(&extract(&a), &extract(&b), pairs)
}

fn baseline_stability_dist_reps(
    ra: &extract::Representations,
    rb: &extract::Representations,
    pairs: &[SourceId],
) -> f64 {
    let divs: Vec<f64> = pairs
        .iter()
        .map(|id| distributional_divergence(&ra.distributions[id], &rb.distributions[id]))
        .collect();
    variance(&divs)
}

fn baseline_stability_dep_reps(
    ra: &extract::Representations,
    rb: &extract::Representations,
) -> f64 {
    match (&ra.mi_matrix, &rb.mi_matrix) {
        (Some(ma), Some(mb)) => mi_matrix_divergence(ma, mb),
        _ => 0.0,
    }
}

fn baseline_stability_spec_reps(
    ra: &extract::Representations,
    rb: &extract::Representations,
    pairs: &[SourceId],
) -> f64 {
    let divs: Vec<f64> = pairs
        .iter()
        .filter_map(|id| Some(spectral_divergence(ra.spectra.get(id)?, rb.spectra.get(id)?)))
        .collect();
    variance(&divs)
}

fn baseline_stability_co_reps(
    ra: &extract::Representations,
    rb: &extract::Representations,
    pairs: &[SourceId],
) -> f64 {
    let divs: Vec<f64> = pairs
        .iter()
        .filter_map(|id| Some(eigen_divergence(ra.eigen.get(id)?, rb.eigen.get(id)?)))
        .collect();
    variance(&divs)
}

fn split_collection(
    collection: &LogCollection,
    pairs: &[SourceId],
) -> (LogCollection, LogCollection) {
    let mut a_sources = HashMap::new();
    let mut b_sources = HashMap::new();

    for &id in pairs {
        let events = &collection.sources[&id].events;
        let mid = events.len() / 2;
        a_sources.insert(id, EventStream { events: events[..mid].to_vec() });
        b_sources.insert(id, EventStream { events: events[mid..].to_vec() });
    }

    let meta = collection.metadata.clone();
    (
        LogCollection { sources: a_sources, metadata: meta.clone() },
        LogCollection { sources: b_sources, metadata: meta },
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn source_pairs(
    baseline: &LogCollection,
    test: &LogCollection,
    mode: &ComparisonMode,
) -> Result<(Vec<SourceId>, MissingSourceReport), DetectError> {
    match mode {
        ComparisonMode::SingleOrigin => {
            let mut baseline_only: Vec<SourceId> = baseline
                .sources
                .keys()
                .filter(|id| !test.sources.contains_key(id))
                .copied()
                .collect();
            let mut test_only: Vec<SourceId> = test
                .sources
                .keys()
                .filter(|id| !baseline.sources.contains_key(id))
                .copied()
                .collect();
            baseline_only.sort_by_key(|s| s.0);
            test_only.sort_by_key(|s| s.0);
            let mut pairs: Vec<SourceId> = baseline
                .sources
                .keys()
                .filter(|id| test.sources.contains_key(id))
                .copied()
                .collect();
            pairs.sort_by_key(|s| s.0);
            Ok((pairs, MissingSourceReport { baseline_only, test_only }))
        }
        ComparisonMode::MultiOrigin => {
            let mut pairs: Vec<SourceId> = baseline
                .sources
                .keys()
                .filter(|id| test.sources.contains_key(id))
                .copied()
                .collect();
            if pairs.is_empty() {
                return Err(DetectError::NoOverlappingSources);
            }
            pairs.sort_by_key(|s| s.0);
            Ok((pairs, MissingSourceReport { baseline_only: vec![], test_only: vec![] }))
        }
    }
}

fn eigen_divergence(a: &analysis::EigenSpectrum, b: &analysis::EigenSpectrum) -> f64 {
    let len = a.eigenvalues.len().min(b.eigenvalues.len());
    if len == 0 {
        return 0.0;
    }
    let a_total: f64 = a.eigenvalues.iter().sum::<f64>().max(1e-10);
    let b_total: f64 = b.eigenvalues.iter().sum::<f64>().max(1e-10);
    let (mut kl_am, mut kl_bm) = (0.0f64, 0.0f64);
    for i in 0..len {
        let p = a.eigenvalues[i] / a_total;
        let q = b.eigenvalues[i] / b_total;
        let m = 0.5 * (p + q);
        if p > 0.0 { kl_am += p * (p / (m + 1e-10)).ln(); }
        if q > 0.0 { kl_bm += q * (q / (m + 1e-10)).ln(); }
    }
    (0.5 * (kl_am + kl_bm) / std::f64::consts::LN_2).clamp(0.0, 1.0)
}

fn variance(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    xs.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / xs.len() as f64
}

fn verdict_string(score: f64, threshold: f64) -> String {
    if score >= threshold {
        format!("fishy score: {score:.2} — something smells off")
    } else {
        format!("fishy score: {score:.2} — looks normal")
    }
}
