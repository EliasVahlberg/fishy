//! Multi-source information-fusion anomaly detection through collection comparison.
//!
//! ```ignore
//! // Single baseline (legacy — z-score path, quarter-split variance):
//! let report = fishy::detect(&[baseline], &test, &DetectConfig::default())?;
//!
//! // Multi-baseline (recommended — empirical CDF, pairwise variance):
//! let report = fishy::detect(&[day1, day2, day3], &test, &DetectConfig::default())?;
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
    bpa_from_zscore, distributional_divergence, ds_combine_many, evidence_bpa, matrix_entropy,
    mi_matrix_divergence, shannon_entropy, spectral_divergence, spectral_entropy, BPA,
    BpaMapping,
};

/// Sigmoid midpoints used when fewer than 3 pairwise baseline samples are available.
const METHOD_MIDPOINTS: [f64; 6] = [2.0, 2.0, 2.5, 2.0, 2.0, 3.0];
use extract::extract;
use std::collections::HashMap;

const METHODS: [&str; 6] = ["dist", "dep", "spec", "co", "conflict", "wavelet"];
const MIN_SPECTRAL_EVENTS: usize = 32;
const GATE_LOW: f64 = 0.05;
const GATE_HIGH: f64 = 0.95;

pub fn detect(
    baselines: &[LogCollection],
    test: &LogCollection,
    config: &DetectConfig,
) -> Result<AnomalyReport, DetectError> {
    if baselines.is_empty() {
        return Err(DetectError::EmptyCollection);
    }
    if test.sources.is_empty() {
        return Err(DetectError::EmptyCollection);
    }

    // Temporal validation against first baseline
    let b_dur = baselines[0].metadata.end_time.saturating_sub(baselines[0].metadata.start_time);
    let t_dur = test.metadata.end_time.saturating_sub(test.metadata.start_time);
    let tol = config.duration_tolerance as f64;
    if tol > 0.0 && b_dur > 0 && t_dur > 0 {
        let ratio = b_dur.max(t_dur) as f64 / b_dur.min(t_dur) as f64;
        if ratio > (1.0 + tol) {
            return Err(DetectError::TemporalMismatch {
                baseline_duration: b_dur,
                test_duration: t_dur,
            });
        }
    }

    // Source pairs: intersection across all baselines and test
    let (pairs, missing) = multi_source_pairs(baselines, test, &config.mode)?;

    match config.strategy {
        FusionStrategy::Adaptive => adaptive(baselines, test, &pairs, &missing, config),
        FusionStrategy::DistributionalFingerprint => {
            adaptive_single(baselines, test, &pairs, &missing, config, 0)
        }
        FusionStrategy::DependencyShift => {
            adaptive_single(baselines, test, &pairs, &missing, config, 1)
        }
        FusionStrategy::SpectralFingerprint => {
            adaptive_single(baselines, test, &pairs, &missing, config, 2)
        }
        FusionStrategy::EvidenceConflict => {
            adaptive_single(baselines, test, &pairs, &missing, config, 4)
        }
    }
}

fn adaptive_single(
    baselines: &[LogCollection],
    test: &LogCollection,
    pairs: &[SourceId],
    missing: &MissingSourceReport,
    config: &DetectConfig,
    method_idx: usize,
) -> Result<AnomalyReport, DetectError> {
    adaptive_inner(baselines, test, pairs, missing, config, Some(method_idx))
}

// ---------------------------------------------------------------------------
// Adaptive fusion — entropy-as-observation pipeline
// ---------------------------------------------------------------------------

fn adaptive(
    baselines: &[LogCollection],
    test: &LogCollection,
    pairs: &[SourceId],
    missing: &MissingSourceReport,
    config: &DetectConfig,
) -> Result<AnomalyReport, DetectError> {
    adaptive_inner(baselines, test, pairs, missing, config, None)
}

fn adaptive_inner(
    baselines: &[LogCollection],
    test: &LogCollection,
    pairs: &[SourceId],
    missing: &MissingSourceReport,
    config: &DetectConfig,
    only_method: Option<usize>,
) -> Result<AnomalyReport, DetectError> {
    // Pre-extract all baseline representations once — reused by outlier rejection,
    // pairwise stats, min-divergence scoring, and trend detection.
    #[cfg(feature = "parallel")]
    let b_reps: Vec<extract::Representations> = {
        use rayon::prelude::*;
        baselines.par_iter().map(extract).collect()
    };
    #[cfg(not(feature = "parallel"))]
    let b_reps: Vec<extract::Representations> = baselines.iter().map(extract).collect();

    let (rejected, active_idx) = reject_outlier_baselines(&b_reps, pairs);
    let active_reps: Vec<&extract::Representations> = active_idx.iter().map(|&i| &b_reps[i]).collect();
    let active_baselines: Vec<&LogCollection> = active_idx.iter().map(|&i| &baselines[i]).collect();
    let baseline = active_baselines[0];
    let b_rep = active_reps[0];

    // Extract test once
    let t_rep = extract(test);

    // 3. Baseline entropy + applicability gate (from reference baseline)
    let b_entropies = method_entropies(&b_rep, pairs);
    let max_entropies = method_max_entropies(&b_rep, pairs);
    let applicable: [bool; 6] = std::array::from_fn(|i| {
        if let Some(m) = only_method { if i != m { return false; } }
        if i == 4 { return pairs.len() >= 2; }
        let norm = if max_entropies[i] > 1e-10 { b_entropies[i] / max_entropies[i] } else { 0.0 };
        norm > GATE_LOW && norm < GATE_HIGH && has_enough_events(i, baseline, pairs)
    });

    // 4. Method divergences: minimum test-to-baseline divergence per method.
    let observations: [(f64, f64); 6] = {
        let mut min_obs: [(f64, f64); 6] = [(f64::MAX, f64::MAX); 6];
        for b_rep_i in &active_reps {
            let b_source_scores: HashMap<SourceId, SourceReport> = {
                let mut ss = HashMap::new();
                for &id in &missing.baseline_only {
                    ss.insert(id, SourceReport { divergence: 1.0, contribution: 1.0, top_events: vec![] });
                }
                for &id in pairs {
                    if b_rep_i.distributions.contains_key(&id) && t_rep.distributions.contains_key(&id) {
                        let div = distributional_divergence(&b_rep_i.distributions[&id], &t_rep.distributions[&id]);
                        let top_events = extract::jsd_contributions(&b_rep_i.distributions[&id], &t_rep.distributions[&id])
                            .into_iter().take(10).collect();
                        ss.insert(id, SourceReport { divergence: div, contribution: 0.0, top_events });
                    }
                }
                ss
            };
            let obs = method_divergences_and_deltas(b_rep_i, &t_rep, pairs, &b_source_scores, &config.source_weights);
            for m in 0..6 {
                if obs[m].0 < min_obs[m].0 { min_obs[m] = obs[m]; }
            }
        }
        std::array::from_fn(|m| if min_obs[m].0 == f64::MAX { (0.0, 0.0) } else { min_obs[m] })
    };

    // Source scores from reference baseline (first active) for reporting
    let mut source_scores: HashMap<SourceId, SourceReport> = HashMap::new();
    for &id in &missing.baseline_only {
        source_scores.insert(id, SourceReport { divergence: 1.0, contribution: 1.0, top_events: vec![] });
    }
    for &id in pairs {
        if b_rep.distributions.contains_key(&id) && t_rep.distributions.contains_key(&id) {
            let div = distributional_divergence(&b_rep.distributions[&id], &t_rep.distributions[&id]);
            let top_events = extract::jsd_contributions(&b_rep.distributions[&id], &t_rep.distributions[&id])
                .into_iter().take(10).collect();
            source_scores.insert(id, SourceReport { divergence: div, contribution: 0.0, top_events });
        }
    }

    // 5. Baseline variance: pairwise (≥3 baselines → ≥3 samples) or quarter-split fallback.
    // Empirical CDF requires ≥3 samples to be meaningful; 2 baselines gives only 1 pair.
    let (div_samples, dh_samples): ([Vec<f64>; 6], [Vec<f64>; 6]) =
        if active_reps.len() >= 3 {
            pairwise_baseline_stats(&active_reps, pairs)
        } else {
            // 1-2 baselines: quarter-split of first baseline. Encode as [mean, std] (len=2)
            // so the sigmoid fallback path is triggered in the BPA loop.
            let stats = multi_split_baseline_stats(baseline, pairs);
            (
                std::array::from_fn(|i| vec![stats[i].0, stats[i].1.sqrt()]),
                std::array::from_fn(|i| vec![stats[i].2, stats[i].3.sqrt()]),
            )
        };

    // 6. Trend signals (only meaningful with ≥3 ordered baselines)
    let trend_zs = if active_reps.len() >= 3 {
        trend_signals(&active_reps, &t_rep, pairs)
    } else {
        [None; 6]
    };

    // 7. BPA construction: empirical CDF (≥3 samples) or sigmoid fallback
    let mut bpas = Vec::new();
    let mut details = Vec::new();

    for i in 0..6 {
        let (div, dh) = observations[i];
        let (z_d, z_dh, pct_d, pct_dh) = if applicable[i] {
            let (mean_d, var_d, mean_dh, var_dh) = mean_var(&div_samples[i], &dh_samples[i]);
            let zd = zscore(div, mean_d, var_d);
            let zdh = zscore(dh.abs(), mean_dh, var_dh);

            if div_samples[i].len() >= 3 {
                // Empirical CDF path — both divergence and ΔH are reliable
                let pd = empirical_commitment(&div_samples[i], div);
                let pdh = empirical_commitment(&dh_samples[i], dh.abs());
                if pd > 0.5 { bpas.push(BPA { anomalous: pd, normal: 0.0, uncertain: 1.0 - pd }); }
                if i != 4 && pdh > 0.5 { bpas.push(BPA { anomalous: pdh, normal: 0.0, uncertain: 1.0 - pdh }); }
                (zd, zdh, Some(pd), Some(pdh))
            } else {
                // Sigmoid fallback — skip ΔH BPAs (within-collection variance underestimates
                // between-collection entropy variance, causing false positives)
                let mapping = BpaMapping::Sigmoid { midpoint: METHOD_MIDPOINTS[i] };
                if zd > 0.0 { bpas.push(bpa_from_zscore(zd, &mapping)); }
                (zd, zdh, None, None)
            }
        } else {
            (0.0, 0.0, None, None)
        };

        details.push(MethodDetail {
            name: METHODS[i].to_string(),
            applicable: applicable[i],
            divergence: div,
            entropy_delta: dh,
            baseline_entropy: b_entropies[i],
            divergence_percentile: pct_d,
            entropy_delta_percentile: pct_dh,
            z_divergence: z_d,
            z_entropy_delta: z_dh,
            trend_z: trend_zs[i],
        });
    }

    // 7. Fuse
    let fused = if bpas.is_empty() {
        BPA { normal: 0.0, anomalous: 0.0, uncertain: 1.0 }
    } else {
        ds_combine_many(&bpas)
    };

    let score = fused.anomalous.clamp(0.0, 1.0);
    let uncertainty = fused.uncertain;

    let meta_conflict = if bpas.len() < 2 { 0.0 } else {
        let mut total = 0.0f64;
        let mut count = 0usize;
        for i in 0..bpas.len() {
            for j in i + 1..bpas.len() {
                total += analysis::ds_conflict(&bpas[i], &bpas[j]);
                count += 1;
            }
        }
        total / count as f64
    };

    for report in source_scores.values_mut() {
        report.contribution = report.divergence * score;
    }

    let pair_scores = compute_pair_scores(&b_rep, &t_rep, pairs);
    let verdict = verdict_string(score, config.significance_threshold as f64);

    Ok(AnomalyReport {
        score,
        uncertainty,
        verdict,
        source_scores,
        pair_scores,
        missing_sources: missing.clone(),
        meta_conflict,
        methods: details,
        baseline_count: active_reps.len(),
        rejected_baselines: rejected,
    })
}

// ---------------------------------------------------------------------------
// Per-method entropy (from baseline representations)
// ---------------------------------------------------------------------------

fn method_entropies(rep: &extract::Representations, _pairs: &[SourceId]) -> [f64; 6] {
    let dist = mean_of(rep.distributions.values().map(|d| {
        let t = d.total.max(1) as f64;
        let probs: Vec<f64> = d.counts.values().map(|&c| c as f64 / t).collect();
        shannon_entropy(&probs)
    }));
    let dep = rep.mi_matrix.as_ref().map(|m| matrix_entropy(m)).unwrap_or(0.0);
    let spec = mean_of(rep.spectra.values().map(|s| spectral_entropy(s)));
    let co = mean_of(rep.eigen.values().map(|e| {
        let total: f64 = e.eigenvalues.iter().sum();
        if total < 1e-10 { 0.0 } else {
            let norm: Vec<f64> = e.eigenvalues.iter().map(|&v| v / total).collect();
            shannon_entropy(&norm)
        }
    }));
    let conflict = 0.5; // no intrinsic entropy; always passes gate via special case
    let wav = mean_of(rep.wavelets.values().filter_map(|wc| {
        let energies: Vec<f64> = wc.levels.iter().map(|l| l.iter().map(|x| x * x).sum::<f64>()).collect();
        let total: f64 = energies.iter().sum();
        if total < 1e-10 { return None; }
        let norm: Vec<f64> = energies.iter().map(|&e| e / total).collect();
        Some(shannon_entropy(&norm))
    }));
    [dist, dep, spec, co, conflict, wav]
}

/// Maximum possible entropy per method (ln(N) where N = number of categories).
fn method_max_entropies(rep: &extract::Representations, _pairs: &[SourceId]) -> [f64; 6] {
    let dist_n: usize = rep.distributions.values().map(|d| d.counts.len()).max().unwrap_or(0);
    let dep_n = rep.mi_matrix.as_ref().map(|m| { let n = m.sources.len(); n * (n - 1) / 2 }).unwrap_or(0);
    let spec_n = rep.spectra.values().map(|s| s.magnitudes.len()).max().unwrap_or(0);
    let co_n = rep.eigen.values().map(|e| e.eigenvalues.len()).max().unwrap_or(0);
    let wav_n = rep.wavelets.values().map(|w| w.levels.len()).max().unwrap_or(0);
    [
        if dist_n > 1 { (dist_n as f64).ln() } else { 0.0 },
        if dep_n > 1 { (dep_n as f64).ln() } else { 0.0 },
        if spec_n > 1 { (spec_n as f64).ln() } else { 0.0 },
        if co_n > 1 { (co_n as f64).ln() } else { 0.0 },
        1.0, // conflict: placeholder
        if wav_n > 1 { (wav_n as f64).ln() } else { 0.0 },
    ]
}

fn has_enough_events(method_idx: usize, baseline: &LogCollection, pairs: &[SourceId]) -> bool {
    match method_idx {
        0 => true, // dist: always ok
        1 => pairs.len() >= 2, // dep: need ≥2 sources
        2 | 3 | 5 => pairs.iter().any(|id| {
            baseline.sources.get(id).map(|s| s.events.len() >= MIN_SPECTRAL_EVENTS).unwrap_or(false)
        }),
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// Per-method divergence + entropy delta
// ---------------------------------------------------------------------------

fn method_divergences_and_deltas(
    ra: &extract::Representations,
    rb: &extract::Representations,
    pairs: &[SourceId],
    source_scores: &HashMap<SourceId, SourceReport>,
    source_weights: &Option<HashMap<SourceId, f32>>,
) -> [(f64, f64); 6] {
    let dist_div = {
        let missing_max: f64 = if source_scores.values().any(|s| s.divergence >= 1.0) { 1.0 } else { 0.0 };
        let paired = if let Some(weights) = source_weights {
            // Weighted mean when weights are provided
            let (wsum, wtotal) = pairs.iter()
                .filter_map(|id| {
                    let d = source_scores.get(id)?.divergence;
                    let w = *weights.get(id).unwrap_or(&1.0) as f64;
                    Some((d * w, w))
                })
                .fold((0.0f64, 0.0f64), |(s, t), (d, w)| (s + d, t + w));
            if wtotal > 0.0 { wsum / wtotal } else { 0.0 }
        } else {
            pairs.iter()
                .filter_map(|id| source_scores.get(id).map(|s| s.divergence))
                .fold(0.0f64, f64::max)
        };
        missing_max.max(paired)
    };
    let dist_dh = dist_entropy(rb) - dist_entropy(ra);

    let dep_div = match (&ra.mi_matrix, &rb.mi_matrix) {
        (Some(ma), Some(mb)) => mi_matrix_divergence(ma, mb),
        _ => 0.0,
    };
    let dep_dh = match (&ra.mi_matrix, &rb.mi_matrix) {
        (Some(ma), Some(mb)) => matrix_entropy(mb) - matrix_entropy(ma),
        _ => 0.0,
    };

    let spec_div = pairs.iter()
        .filter_map(|id| Some(spectral_divergence(ra.spectra.get(id)?, rb.spectra.get(id)?)))
        .fold(0.0f64, f64::max);
    let spec_dh = mean_of(rb.spectra.values().map(|s| spectral_entropy(s)))
        - mean_of(ra.spectra.values().map(|s| spectral_entropy(s)));

    let co_div = pairs.iter()
        .filter_map(|id| Some(eigen_divergence(ra.eigen.get(id)?, rb.eigen.get(id)?)))
        .fold(0.0f64, f64::max);
    let co_dh = eigen_entropy_mean(rb) - eigen_entropy_mean(ra);

    let conflict_div = {
        let bpas: Vec<BPA> = pairs.iter()
            .filter_map(|id| source_scores.get(id).map(|s| evidence_bpa(s.divergence, 1.0)))
            .collect();
        pairwise_max_conflict(&bpas)
    };

    let wav_div = pairs.iter()
        .filter_map(|id| Some(wavelet_divergence(ra.wavelets.get(id)?, rb.wavelets.get(id)?)))
        .fold(0.0f64, f64::max);
    let wav_dh = wavelet_entropy_mean(rb) - wavelet_entropy_mean(ra);

    [
        (dist_div, dist_dh),
        (dep_div, dep_dh),
        (spec_div, spec_dh),
        (co_div, co_dh),
        (conflict_div, 0.0),
        (wav_div, wav_dh),
    ]
}

// ---------------------------------------------------------------------------
// Multi-split baseline statistics (single-baseline fallback)
// ---------------------------------------------------------------------------

/// Returns per-method (mean_d, var_d, mean_|ΔH|, var_|ΔH|) from 3 quarter-pair splits.
fn multi_split_baseline_stats(baseline: &LogCollection, pairs: &[SourceId]) -> [(f64, f64, f64, f64); 6] {
    if pairs.is_empty() {
        return [(0.0, 0.0, 0.0, 0.0); 6];
    }

    let q = quarter_split(baseline, pairs);
    // 3 complementary half-pair partitions
    let partition_indices: [(usize, usize, usize, usize); 3] = [
        (0, 1, 2, 3), // (Q0+Q1) vs (Q2+Q3)
        (0, 2, 1, 3), // (Q0+Q2) vs (Q1+Q3)
        (0, 3, 1, 2), // (Q0+Q3) vs (Q1+Q2)
    ];

    #[cfg(feature = "parallel")]
    let samples = {
        let (s0, (s1, s2)) = join(
            || split_scores(&q, partition_indices[0], pairs),
            || join(
                || split_scores(&q, partition_indices[1], pairs),
                || split_scores(&q, partition_indices[2], pairs),
            ),
        );
        [s0, s1, s2]
    };
    #[cfg(not(feature = "parallel"))]
    let samples = partition_indices.map(|p| split_scores(&q, p, pairs));

    std::array::from_fn(|m| {
        let ds: [f64; 3] = std::array::from_fn(|s| samples[s][m].0);
        let dhs: [f64; 3] = std::array::from_fn(|s| samples[s][m].1.abs());
        (mean_arr(&ds), variance_arr(&ds), mean_arr(&dhs), variance_arr(&dhs))
    })
}

fn split_scores(
    q: &[LogCollection; 4],
    (a, b, c, d): (usize, usize, usize, usize),
    pairs: &[SourceId],
) -> [(f64, f64); 6] {
    let ha = merge_collections(&q[a], &q[b]);
    let hb = merge_collections(&q[c], &q[d]);
    let ra = extract(&ha);
    let rb = extract(&hb);
    // Compute per-source divergences for conflict method
    let source_divs: HashMap<SourceId, SourceReport> = pairs.iter().map(|&id| {
        let div = distributional_divergence(&ra.distributions[&id], &rb.distributions[&id]);
        (id, SourceReport { divergence: div, contribution: 0.0, top_events: vec![] })
    }).collect();
    method_divergences_and_deltas(&ra, &rb, pairs, &source_divs, &None)
}

fn quarter_split(collection: &LogCollection, pairs: &[SourceId]) -> [LogCollection; 4] {
    let meta = collection.metadata.clone();
    let mut qs: [HashMap<SourceId, EventStream>; 4] = Default::default();
    for &id in pairs {
        let events = &collection.sources[&id].events;
        let q = events.len() / 4;
        qs[0].insert(id, EventStream { events: events[..q].to_vec() });
        qs[1].insert(id, EventStream { events: events[q..2 * q].to_vec() });
        qs[2].insert(id, EventStream { events: events[2 * q..3 * q].to_vec() });
        qs[3].insert(id, EventStream { events: events[3 * q..].to_vec() });
    }
    qs.map(|sources| LogCollection { sources, metadata: meta.clone() })
}

fn merge_collections(a: &LogCollection, b: &LogCollection) -> LogCollection {
    let mut sources = HashMap::new();
    for (id, stream) in &a.sources {
        let mut events = stream.events.clone();
        if let Some(other) = b.sources.get(id) {
            events.extend_from_slice(&other.events);
        }
        sources.insert(*id, EventStream { events });
    }
    LogCollection { sources, metadata: a.metadata.clone() }
}

// ---------------------------------------------------------------------------
// Multi-baseline: pairwise stats, outlier rejection, trend detection
// ---------------------------------------------------------------------------

/// Compute per-method divergence samples from all baseline pairs.
/// Returns ([div_samples; 6], [dh_samples; 6]).
fn pairwise_baseline_stats(
    reps: &[&extract::Representations],
    pairs: &[SourceId],
) -> ([Vec<f64>; 6], [Vec<f64>; 6]) {
    let mut div_samples: [Vec<f64>; 6] = Default::default();
    let mut dh_samples: [Vec<f64>; 6] = Default::default();

    for i in 0..reps.len() {
        for j in i + 1..reps.len() {
            let (ra, rb) = (reps[i], reps[j]);
            let source_divs: HashMap<SourceId, SourceReport> = pairs.iter().map(|&id| {
                let div = if ra.distributions.contains_key(&id) && rb.distributions.contains_key(&id) {
                    distributional_divergence(&ra.distributions[&id], &rb.distributions[&id])
                } else { 1.0 };
                (id, SourceReport { divergence: div, contribution: 0.0, top_events: vec![] })
            }).collect();
            let obs = method_divergences_and_deltas(ra, rb, pairs, &source_divs, &None);
            for m in 0..6 {
                div_samples[m].push(obs[m].0);
                dh_samples[m].push(obs[m].1.abs());
            }
        }
    }
    (div_samples, dh_samples)
}

/// Empirical CDF: fraction of samples strictly less than value.
/// Using strict inequality prevents a test value equal to a pairwise sample from scoring 1.0.
/// Returns 0.5 if no samples (maximally uncertain).
fn empirical_commitment(samples: &[f64], value: f64) -> f64 {
    if samples.is_empty() { return 0.5; }
    let count = samples.iter().filter(|&&s| s < value).count();
    count as f64 / samples.len() as f64
}

/// Reject baselines whose mean pairwise divergence to others is >2σ above the group mean.
/// Returns (rejected_indices, active_baselines).
fn reject_outlier_baselines(
    reps: &[extract::Representations],
    pairs: &[SourceId],
) -> (Vec<usize>, Vec<usize>) {
    if reps.len() < 3 {
        return (vec![], (0..reps.len()).collect());
    }

    let mean_divs: Vec<f64> = (0..reps.len()).map(|i| {
        let mut total = 0.0f64;
        let mut count = 0usize;
        for j in 0..reps.len() {
            if i == j { continue; }
            let (ra, rb) = (&reps[i], &reps[j]);
            let source_divs: HashMap<SourceId, SourceReport> = pairs.iter().map(|&id| {
                let div = if ra.distributions.contains_key(&id) && rb.distributions.contains_key(&id) {
                    distributional_divergence(&ra.distributions[&id], &rb.distributions[&id])
                } else { 1.0 };
                (id, SourceReport { divergence: div, contribution: 0.0, top_events: vec![] })
            }).collect();
            let obs = method_divergences_and_deltas(ra, rb, pairs, &source_divs, &None);
            total += obs.iter().map(|(d, _)| d).sum::<f64>() / 6.0;
            count += 1;
        }
        if count > 0 { total / count as f64 } else { 0.0 }
    }).collect();

    let group_mean = mean_divs.iter().sum::<f64>() / mean_divs.len() as f64;
    let group_std = {
        let var = mean_divs.iter().map(|d| (d - group_mean).powi(2)).sum::<f64>() / mean_divs.len() as f64;
        var.sqrt()
    };
    let threshold = group_mean + 2.0 * group_std;

    let mut rejected = vec![];
    let mut active = vec![];
    for i in 0..reps.len() {
        if group_std > 1e-10 && mean_divs[i] > threshold {
            rejected.push(i);
        } else {
            active.push(i);
        }
    }
    if active.is_empty() { active = (0..reps.len()).collect(); }
    (rejected, active)
}

/// Trend detection: for each method, fit linear regression on divergences of ordered
/// baselines against the first baseline, then compute z-score of test divergence
/// relative to the extrapolated prediction.
fn trend_signals(
    reps: &[&extract::Representations],
    t_rep: &extract::Representations,
    pairs: &[SourceId],
) -> [Option<f64>; 6] {
    let ref_rep = reps[0];

    let baseline_divs: Vec<[f64; 6]> = (1..reps.len()).map(|i| {
        let rb = reps[i];
        let source_divs: HashMap<SourceId, SourceReport> = pairs.iter().map(|&id| {
            let div = if ref_rep.distributions.contains_key(&id) && rb.distributions.contains_key(&id) {
                distributional_divergence(&ref_rep.distributions[&id], &rb.distributions[&id])
            } else { 1.0 };
            (id, SourceReport { divergence: div, contribution: 0.0, top_events: vec![] })
        }).collect();
        let obs = method_divergences_and_deltas(ref_rep, rb, pairs, &source_divs, &None);
        std::array::from_fn(|m| obs[m].0)
    }).collect();

    let test_source_divs: HashMap<SourceId, SourceReport> = pairs.iter().map(|&id| {
        let div = if ref_rep.distributions.contains_key(&id) && t_rep.distributions.contains_key(&id) {
            distributional_divergence(&ref_rep.distributions[&id], &t_rep.distributions[&id])
        } else { 1.0 };
        (id, SourceReport { divergence: div, contribution: 0.0, top_events: vec![] })
    }).collect();
    let test_obs = method_divergences_and_deltas(ref_rep, t_rep, pairs, &test_source_divs, &None);

    std::array::from_fn(|m| {
        let n = baseline_divs.len();
        if n < 2 { return None; }
        // xs = 1..n (baseline indices), ys = divergences
        let xs: Vec<f64> = (1..=n).map(|i| i as f64).collect();
        let ys: Vec<f64> = baseline_divs.iter().map(|d| d[m]).collect();
        let (slope, intercept, residual_std) = linear_regression(&xs, &ys);
        let predicted = slope * (n + 1) as f64 + intercept;
        if residual_std < 1e-10 { return None; }
        Some((test_obs[m].0 - predicted) / residual_std)
    })
}

/// Simple OLS linear regression. Returns (slope, intercept, residual_std).
fn linear_regression(xs: &[f64], ys: &[f64]) -> (f64, f64, f64) {
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let ss_xx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let ss_xy: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = if ss_xx > 1e-10 { ss_xy / ss_xx } else { 0.0 };
    let intercept = my - slope * mx;
    let residuals: Vec<f64> = xs.iter().zip(ys.iter())
        .map(|(x, y)| y - (slope * x + intercept)).collect();
    let res_var = residuals.iter().map(|r| r.powi(2)).sum::<f64>() / n;
    (slope, intercept, res_var.sqrt())
}

fn mean_var(div_samples: &[f64], dh_samples: &[f64]) -> (f64, f64, f64, f64) {
    // Single-baseline encoding: [mean, std] stored as 2-element vec
    let (mean_d, var_d) = if div_samples.len() == 2 {
        (div_samples[0], div_samples[1].powi(2))
    } else if div_samples.is_empty() {
        (0.0, 0.0)
    } else {
        let m = div_samples.iter().sum::<f64>() / div_samples.len() as f64;
        let v = div_samples.iter().map(|x| (x - m).powi(2)).sum::<f64>() / div_samples.len() as f64;
        (m, v)
    };
    let (mean_dh, var_dh) = if dh_samples.len() == 2 {
        (dh_samples[0], dh_samples[1].powi(2))
    } else if dh_samples.is_empty() {
        (0.0, 0.0)
    } else {
        let m = dh_samples.iter().sum::<f64>() / dh_samples.len() as f64;
        let v = dh_samples.iter().map(|x| (x - m).powi(2)).sum::<f64>() / dh_samples.len() as f64;
        (m, v)
    };
    (mean_d, var_d, mean_dh, var_dh)
}

/// Source pairs across multiple baselines: intersection of all baselines ∩ test (SO),
/// or union of baselines ∩ test (MO).
fn multi_source_pairs(
    baselines: &[LogCollection],
    test: &LogCollection,
    mode: &ComparisonMode,
) -> Result<(Vec<SourceId>, MissingSourceReport), DetectError> {
    // Use first baseline as reference for missing-source reporting
    source_pairs(&baselines[0], test, mode)
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
            let mut baseline_only: Vec<SourceId> = baseline.sources.keys()
                .filter(|id| !test.sources.contains_key(id)).copied().collect();
            let mut test_only: Vec<SourceId> = test.sources.keys()
                .filter(|id| !baseline.sources.contains_key(id)).copied().collect();
            baseline_only.sort_by_key(|s| s.0);
            test_only.sort_by_key(|s| s.0);
            let mut pairs: Vec<SourceId> = baseline.sources.keys()
                .filter(|id| test.sources.contains_key(id)).copied().collect();
            pairs.sort_by_key(|s| s.0);
            Ok((pairs, MissingSourceReport { baseline_only, test_only }))
        }
        ComparisonMode::MultiOrigin => {
            let mut pairs: Vec<SourceId> = baseline.sources.keys()
                .filter(|id| test.sources.contains_key(id)).copied().collect();
            if pairs.is_empty() { return Err(DetectError::NoOverlappingSources); }
            pairs.sort_by_key(|s| s.0);
            Ok((pairs, MissingSourceReport { baseline_only: vec![], test_only: vec![] }))
        }
    }
}

fn compute_pair_scores(
    b_rep: &extract::Representations,
    t_rep: &extract::Representations,
    pairs: &[SourceId],
) -> Vec<PairReport> {
    if pairs.len() < 2 { return vec![]; }
    match (&b_rep.mi_matrix, &t_rep.mi_matrix) {
        (Some(bm), Some(tm)) => {
            let b_idx: HashMap<SourceId, usize> = bm.sources.iter().enumerate().map(|(i, &id)| (id, i)).collect();
            let t_idx: HashMap<SourceId, usize> = tm.sources.iter().enumerate().map(|(i, &id)| (id, i)).collect();
            let mut out = vec![];
            for i in 0..pairs.len() {
                for j in i + 1..pairs.len() {
                    let (a, b) = (pairs[i], pairs[j]);
                    let b_mi = b_idx.get(&a).and_then(|&ai| b_idx.get(&b).map(|&bi| bm.values[ai][bi])).unwrap_or(0.0);
                    let t_mi = t_idx.get(&a).and_then(|&ai| t_idx.get(&b).map(|&bi| tm.values[ai][bi])).unwrap_or(0.0);
                    let denom = b_mi.max(t_mi);
                    let dep_shift = if denom < 1e-10 { 0.0 } else { (b_mi - t_mi).abs() / denom };
                    out.push(PairReport { source_a: a, source_b: b, dependency_shift: dep_shift, baseline_correlation: b_mi, test_correlation: t_mi });
                }
            }
            out
        }
        _ => vec![],
    }
}

fn eigen_divergence(a: &analysis::EigenSpectrum, b: &analysis::EigenSpectrum) -> f64 {
    let len = a.eigenvalues.len().min(b.eigenvalues.len());
    if len == 0 { return 0.0; }
    let a_total = a.eigenvalues.iter().sum::<f64>().max(1e-10);
    let b_total = b.eigenvalues.iter().sum::<f64>().max(1e-10);
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

fn wavelet_divergence(a: &analysis::WaveletCoefficients, b: &analysis::WaveletCoefficients) -> f64 {
    a.levels.iter().zip(b.levels.iter()).map(|(al, bl)| {
        let ae: f64 = al.iter().map(|x| x * x).sum();
        let be: f64 = bl.iter().map(|x| x * x).sum();
        let denom = ae.max(be);
        if denom < 1e-10 { 0.0 } else { (ae - be).abs() / denom }
    }).fold(0.0f64, f64::max)
}

fn dist_entropy(rep: &extract::Representations) -> f64 {
    mean_of(rep.distributions.values().map(|d| {
        let t = d.total.max(1) as f64;
        let probs: Vec<f64> = d.counts.values().map(|&c| c as f64 / t).collect();
        shannon_entropy(&probs)
    }))
}

fn eigen_entropy_mean(rep: &extract::Representations) -> f64 {
    mean_of(rep.eigen.values().map(|e| {
        let total: f64 = e.eigenvalues.iter().sum();
        if total < 1e-10 { 0.0 } else {
            let norm: Vec<f64> = e.eigenvalues.iter().map(|&v| v / total).collect();
            shannon_entropy(&norm)
        }
    }))
}

fn wavelet_entropy_mean(rep: &extract::Representations) -> f64 {
    mean_of(rep.wavelets.values().filter_map(|wc| {
        let energies: Vec<f64> = wc.levels.iter().map(|l| l.iter().map(|x| x * x).sum::<f64>()).collect();
        let total: f64 = energies.iter().sum();
        if total < 1e-10 { return None; }
        let norm: Vec<f64> = energies.iter().map(|&e| e / total).collect();
        Some(shannon_entropy(&norm))
    }))
}

fn pairwise_max_conflict(bpas: &[BPA]) -> f64 {
    let mut max = 0.0f64;
    for i in 0..bpas.len() {
        for j in i + 1..bpas.len() {
            max = max.max(analysis::ds_conflict(&bpas[i], &bpas[j]));
        }
    }
    max
}

fn zscore(observation: f64, mean: f64, variance: f64) -> f64 {
    let sigma = variance.sqrt();
    if sigma < 1e-10 { 0.0 } else { (observation - mean) / sigma }
}

fn mean_of(iter: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = iter.fold((0.0f64, 0usize), |(s, c), v| (s + v, c + 1));
    if count == 0 { 0.0 } else { sum / count as f64 }
}

fn mean_arr(xs: &[f64; 3]) -> f64 { (xs[0] + xs[1] + xs[2]) / 3.0 }

fn variance_arr(xs: &[f64; 3]) -> f64 {
    let m = mean_arr(xs);
    ((xs[0] - m).powi(2) + (xs[1] - m).powi(2) + (xs[2] - m).powi(2)) / 3.0
}

fn verdict_string(score: f64, _threshold: f64) -> String {
    let tier = match score {
        s if s < 0.20 => "looks clean",
        s if s < 0.40 => "probably fine",
        s if s < 0.60 => "worth a look",
        s if s < 0.80 => "something smells off",
        _             => "definitely fishy",
    };
    format!("fishy score: {score:.2} — {tier}")
}
