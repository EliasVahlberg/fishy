//! Log file tokenizer and encoder for fishy.
//!
//! Converts raw log files into the `LogCollection` JSON format that fishy expects.
//!
//! # Workflow
//!
//! ```text
//! encoder build-dict logs/baseline/ -o dict.json
//! encoder encode logs/baseline/ --dict dict.json -o collections/baseline/
//! encoder encode logs/test/     --dict dict.json -o collections/test/
//! fishy -b collections/baseline/ -c collections/test/
//! ```

mod dict;
mod parser;
mod tokenizer;

pub use dict::Dictionary;
pub use parser::{LogFormat, LogInput};

use analysis::SourceId;
use fishy::{CollectionMetadata, Event, EventStream, LogCollection};
use std::collections::HashMap;

/// Build a frequency-ranked `Dictionary` by scanning all log lines in `inputs`.
///
/// Template IDs are assigned by descending frequency: the most common template
/// gets `TemplateId(1)`, the next `TemplateId(2)`, and so on. This mirrors the
/// core idea of Huffman coding — frequent symbols get the smallest codes.
pub fn build_dictionary(inputs: &[LogInput]) -> Dictionary {
    let mut freqs: HashMap<String, u64> = HashMap::new();
    for input in inputs {
        if let Ok(content) = std::fs::read_to_string(&input.path) {
            for line in content.lines() {
                if let Some((template, _)) =
                    tokenizer::extract_template_and_ts(line, &input.format)
                {
                    *freqs.entry(template).or_insert(0) += 1;
                }
            }
        }
    }
    Dictionary::from_frequencies(freqs.into_iter().collect())
}

/// Encode a set of log files into a `LogCollection` using an existing `Dictionary`.
///
/// Timestamps are **sticky**: a timestamp seen on one line applies to all subsequent
/// lines (within the same source file) until a new timestamp appears. This matches
/// log formats where a timestamp header precedes a block of events.
///
/// Lines whose templates are not in the dictionary are assigned `TemplateId(0)`.
pub fn encode(inputs: &[LogInput], dict: &Dictionary) -> LogCollection {
    let mut sources: HashMap<SourceId, EventStream> = HashMap::new();
    let mut global_min_ts: Option<u64> = None;
    let mut global_max_ts: Option<u64> = None;

    // First pass: collect (source_id, template_id, absolute_ts) with sticky timestamps.
    let mut raw: Vec<(SourceId, analysis::TemplateId, u64)> = Vec::new();
    for input in inputs {
        let Ok(content) = std::fs::read_to_string(&input.path) else { continue };
        let mut last_ts: Option<u64> = None;
        for line in content.lines() {
            let Some((template, ts_str)) =
                tokenizer::extract_template_and_ts(line, &input.format)
            else {
                continue;
            };
            // Update sticky timestamp when this line carries one.
            if let Some(ts_str) = ts_str {
                if let Some(ts) = parser::parse_timestamp(&ts_str, &input.format) {
                    last_ts = Some(ts);
                    global_min_ts = Some(global_min_ts.map_or(ts, |m: u64| m.min(ts)));
                    global_max_ts = Some(global_max_ts.map_or(ts, |m: u64| m.max(ts)));
                }
            }
            raw.push((input.source_id, dict.lookup(&template), last_ts.unwrap_or(0)));
        }
    }

    let base_ts = global_min_ts.unwrap_or(0);

    // Second pass: build EventStreams with relative timestamps.
    for (source_id, tid, ts) in raw {
        sources
            .entry(source_id)
            .or_insert_with(|| EventStream { events: vec![] })
            .events
            .push(Event {
                template_id: tid,
                timestamp: Some(ts.saturating_sub(base_ts)),
                params: HashMap::new(),
            });
    }

    LogCollection {
        sources,
        metadata: CollectionMetadata {
            start_time: 0,
            end_time: global_max_ts.unwrap_or(0).saturating_sub(base_ts),
        },
    }
}
