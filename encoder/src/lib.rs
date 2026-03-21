//! Log file tokenizer and encoder for fishy.
//!
//! Converts raw log files into the `LogCollection` JSON format that fishy expects.
//!
//! # Workflow
//!
//! ```text
//! # 1. Build a template dictionary from baseline logs
//! encoder build-dict logs/baseline/ -o dict.json
//!
//! # 2. Encode baseline → fishy format (using the dictionary)
//! encoder encode logs/baseline/ --dict dict.json -o collections/baseline/
//!
//! # 3. Encode test using the SAME dictionary (shared template IDs)
//! encoder encode logs/test/ --dict dict.json -o collections/test/
//!
//! # 4. Run fishy
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

/// Build a `Dictionary` by scanning all log lines in `inputs`.
/// Must be called on the baseline before encoding either collection.
pub fn build_dictionary(inputs: &[LogInput]) -> Dictionary {
    let mut dict = Dictionary::new();
    for input in inputs {
        if let Ok(lines) = std::fs::read_to_string(&input.path) {
            for line in lines.lines() {
                if let Some(template) = tokenizer::extract_template(line, &input.format) {
                    dict.intern(template);
                }
            }
        }
    }
    dict
}

/// Encode a set of log files into a `LogCollection` using an existing `Dictionary`.
/// Lines whose templates are not in the dictionary are assigned `TemplateId(0)` (unknown).
pub fn encode(inputs: &[LogInput], dict: &Dictionary) -> LogCollection {
    let mut sources: HashMap<SourceId, EventStream> = HashMap::new();
    let mut global_min_ts: Option<u64> = None;
    let mut global_max_ts: Option<u64> = None;

    // First pass: collect all (source, template_id, timestamp) triples.
    let mut raw: Vec<(SourceId, analysis::TemplateId, u64)> = Vec::new();
    for input in inputs {
        let Ok(content) = std::fs::read_to_string(&input.path) else { continue };
        for line in content.lines() {
            let Some((template, ts)) = tokenizer::extract_template_and_ts(line, &input.format)
            else {
                continue;
            };
            let tid = dict.lookup(&template);
            let ts_secs = parser::parse_timestamp(&ts, &input.format).unwrap_or(0);
            global_min_ts = Some(global_min_ts.map_or(ts_secs, |m: u64| m.min(ts_secs)));
            global_max_ts = Some(global_max_ts.map_or(ts_secs, |m: u64| m.max(ts_secs)));
            raw.push((input.source_id, tid, ts_secs));
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
