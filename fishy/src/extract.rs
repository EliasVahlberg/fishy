use crate::types::{EventStream, LogCollection, SourceId};
use analysis::{
    co_occurrence_spectrum, mutual_information_matrix_timed, spectral_fingerprint,
    wavelet_decompose, EventDistribution, EigenSpectrum, MIMatrix, PowerSpectrum, TemplateId,
    WaveletCoefficients,
};
use std::collections::HashMap;

/// All analysis representations extracted from a collection.
pub struct Representations {
    /// Per-source event distributions.
    pub distributions: HashMap<SourceId, EventDistribution>,
    /// Per-source power spectra (requires timestamps).
    pub spectra: HashMap<SourceId, PowerSpectrum>,
    /// Per-source co-occurrence eigenspectra.
    pub eigen: HashMap<SourceId, EigenSpectrum>,
    /// Per-source wavelet decompositions.
    pub wavelets: HashMap<SourceId, WaveletCoefficients>,
    /// Cross-source MI matrix (None if <2 sources).
    pub mi_matrix: Option<MIMatrix>,
}

/// Bin width for spectral analysis — scales with collection duration.
/// Targets ~1024 bins: duration / 1024, clamped to [1s, 1h].
pub fn adaptive_bin_width(duration_secs: u64) -> u64 {
    (duration_secs / 1024).clamp(1, 3600)
}

/// Co-occurrence window — 1/10 of bin width, minimum 1s.
pub fn adaptive_co_window(bin_width: u64) -> u64 {
    (bin_width / 10).max(1)
}

pub fn extract(collection: &LogCollection) -> Representations {
    let duration = collection.metadata.end_time.saturating_sub(collection.metadata.start_time);
    let bin_width = adaptive_bin_width(duration);
    let co_window = adaptive_co_window(bin_width);
    extract_with(collection, bin_width, co_window)
}

pub fn extract_with(collection: &LogCollection, bin_width: u64, co_window: u64) -> Representations {
    let mut sources: Vec<SourceId> = collection.sources.keys().copied().collect();
    sources.sort_by_key(|s| s.0);

    let distributions = sources
        .iter()
        .map(|&id| (id, to_distribution(&collection.sources[&id])))
        .collect();

    let spectra = sources
        .iter()
        .map(|&id| {
            let times = event_times(&collection.sources[&id]);
            (id, spectral_fingerprint(&times, bin_width))
        })
        .collect();

    let eigen = sources
        .iter()
        .map(|&id| {
            let events = timed_events(&collection.sources[&id]);
            (id, co_occurrence_spectrum(&events, co_window))
        })
        .collect();

    let wavelets = sources
        .iter()
        .map(|&id| {
            let times = event_times(&collection.sources[&id]);
            (id, wavelet_decompose(&times, bin_width, 4))
        })
        .collect();

    let mi_matrix = if sources.len() >= 2 {
        let timed: Vec<Vec<(TemplateId, u64)>> = sources
            .iter()
            .map(|id| timed_events(&collection.sources[id]))
            .collect();
        let col_refs: Vec<(SourceId, &[(TemplateId, u64)])> =
            sources.iter().zip(timed.iter()).map(|(&id, v)| (id, v.as_slice())).collect();
        Some(mutual_information_matrix_timed(&col_refs, bin_width))
    } else {
        None
    };

    Representations { distributions, spectra, eigen, wavelets, mi_matrix }
}

pub fn to_distribution(stream: &EventStream) -> EventDistribution {
    let mut counts = HashMap::new();
    for event in &stream.events {
        *counts.entry(event.template_id).or_insert(0u64) += 1;
    }
    EventDistribution { counts, total: stream.events.len() as u64 }
}

/// Per-template JSD contribution: the sum of the two KL terms for each template.
/// Returns a vec of (template_id, contribution) sorted descending by contribution.
pub fn jsd_contributions(
    baseline: &EventDistribution,
    test: &EventDistribution,
) -> Vec<(TemplateId, f64)> {
    let b_total = (baseline.total.max(1)) as f64;
    let t_total = (test.total.max(1)) as f64;

    // Union of all template IDs.
    let mut ids: Vec<TemplateId> = baseline.counts.keys().chain(test.counts.keys()).copied().collect();
    ids.sort_by_key(|t| t.0);
    ids.dedup();

    let mut out: Vec<(TemplateId, f64)> = ids
        .into_iter()
        .map(|tid| {
            let p = baseline.counts.get(&tid).copied().unwrap_or(0) as f64 / b_total;
            let q = test.counts.get(&tid).copied().unwrap_or(0) as f64 / t_total;
            let m = 0.5 * (p + q);
            let kl_pm = if p > 0.0 { p * (p / (m + 1e-10)).ln() } else { 0.0 };
            let kl_qm = if q > 0.0 { q * (q / (m + 1e-10)).ln() } else { 0.0 };
            (tid, 0.5 * (kl_pm + kl_qm) / std::f64::consts::LN_2)
        })
        .collect();

    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

pub fn event_times(stream: &EventStream) -> Vec<u64> {
    let base = stream.events.iter().filter_map(|e| e.timestamp).min().unwrap_or(0);
    stream.events.iter().filter_map(|e| e.timestamp.map(|t| t.saturating_sub(base))).collect()
}

pub fn timed_events(stream: &EventStream) -> Vec<(TemplateId, u64)> {
    let base = stream.events.iter().filter_map(|e| e.timestamp).min().unwrap_or(0);
    stream
        .events
        .iter()
        .map(|e| (e.template_id, e.timestamp.map(|t| t.saturating_sub(base)).unwrap_or(0)))
        .collect()
}
