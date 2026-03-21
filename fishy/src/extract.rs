use crate::types::{EventStream, LogCollection, SourceId};
use analysis::{
    co_occurrence_spectrum, mutual_information_matrix, spectral_fingerprint, EventDistribution,
    EigenSpectrum, MIMatrix, PowerSpectrum, TemplateId,
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

/// Bin width for spectral analysis (seconds).
pub const BIN_WIDTH: u64 = 60;
/// Co-occurrence window (seconds).
pub const CO_WINDOW: u64 = 30;

pub fn extract(collection: &LogCollection) -> Representations {
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
            (id, spectral_fingerprint(&times, BIN_WIDTH))
        })
        .collect();

    let eigen = sources
        .iter()
        .map(|&id| {
            let events = timed_events(&collection.sources[&id]);
            (id, co_occurrence_spectrum(&events, CO_WINDOW))
        })
        .collect();

    let (mi_matrix, mi_sources) = if sources.len() >= 2 {
        let cols: Vec<Vec<TemplateId>> = sources
            .iter()
            .map(|id| collection.sources[id].events.iter().map(|e| e.template_id).collect())
            .collect();
        let col_refs: Vec<(SourceId, &[TemplateId])> =
            sources.iter().zip(cols.iter()).map(|(&id, v)| (id, v.as_slice())).collect();
        let m = mutual_information_matrix(&col_refs);
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
