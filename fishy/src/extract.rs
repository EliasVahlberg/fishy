use crate::types::{EventStream, LogCollection, SourceId};
use analysis::{
    co_occurrence_spectrum, mutual_information_matrix_timed, spectral_fingerprint,
    EventDistribution, EigenSpectrum, MIMatrix, PowerSpectrum, TemplateId,
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
    /// Cross-source MI matrix (None if <2 sources).
    pub mi_matrix: Option<MIMatrix>,
    /// Ordered source list used to build the MI matrix (reserved for future use).
    #[allow(dead_code)]
    pub mi_sources: Vec<SourceId>,
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

    let (mi_matrix, mi_sources) = if sources.len() >= 2 {
        let timed: Vec<Vec<(TemplateId, u64)>> = sources
            .iter()
            .map(|id| timed_events(&collection.sources[id]))
            .collect();
        let col_refs: Vec<(SourceId, &[(TemplateId, u64)])> =
            sources.iter().zip(timed.iter()).map(|(&id, v)| (id, v.as_slice())).collect();
        let m = mutual_information_matrix_timed(&col_refs, bin_width);
        (Some(m), sources.clone())
    } else {
        (None, vec![])
    };

    Representations { distributions, spectra, eigen, mi_matrix, mi_sources }
}

pub fn to_distribution(stream: &EventStream) -> EventDistribution {
    let mut counts = HashMap::new();
    for event in &stream.events {
        *counts.entry(event.template_id).or_insert(0u64) += 1;
    }
    EventDistribution { counts, total: stream.events.len() as u64 }
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
