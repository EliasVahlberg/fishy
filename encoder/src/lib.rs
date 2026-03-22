//! Log file tokenizer and encoder for fishy.
//!
//! Converts raw log files into the `LogCollection` JSON format that fishy expects.
//! Uses a Drain parse tree for format-agnostic template extraction.
//!
//! # Workflow
//!
//! ```text
//! encoder build-dict logs/baseline/ -o dict.json    # also writes drain.json
//! encoder encode logs/baseline/ --dict dict.json -o collections/baseline/
//! encoder encode logs/test/     --dict dict.json -o collections/test/
//! fishy -b collections/baseline/ -c collections/test/
//! ```

mod dict;
mod drain;
mod parser;

pub use dict::Dictionary;
pub use drain::DrainTree;
pub use parser::LogInput;

use analysis::SourceId;
use fishy::{CollectionMetadata, Event, EventStream, LogCollection};
use std::collections::HashMap;

/// Build a `DrainTree` by training on all log lines in `inputs`.
pub fn build_drain_tree(inputs: &[LogInput], sim_threshold: f64, max_children: usize) -> DrainTree {
    let mut tree = DrainTree::new(sim_threshold, max_children);
    for input in inputs {
        for path in &input.paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let (_ts, rest) = parser::extract_timestamp(line);
                    tree.train(rest);
                }
            }
        }
    }
    tree
}

/// Build a frequency-ranked `Dictionary` using a trained `DrainTree`.
pub fn build_dictionary(inputs: &[LogInput], tree: &DrainTree) -> Dictionary {
    let mut freqs: HashMap<String, u64> = HashMap::new();
    for input in inputs {
        for path in &input.paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let (_ts, rest) = parser::extract_timestamp(line);
                    let template = tree.classify(rest);
                    if !template.is_empty() {
                        *freqs.entry(template).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    Dictionary::from_frequencies(freqs.into_iter().collect())
}

/// Encode log files into a `LogCollection` using a trained `DrainTree` and `Dictionary`.
///
/// Timestamps use sticky model: a timestamp seen on one line applies to subsequent
/// lines until a new timestamp appears.
pub fn encode(inputs: &[LogInput], tree: &DrainTree, dict: &Dictionary) -> LogCollection {
    let mut sources: HashMap<SourceId, EventStream> = HashMap::new();
    let mut global_min_ts: Option<u64> = None;
    let mut global_max_ts: Option<u64> = None;

    let mut raw: Vec<(SourceId, analysis::TemplateId, u64)> = Vec::new();
    for input in inputs {
        let mut last_ts: Option<u64> = None;
        for path in &input.paths {
            let Ok(content) = std::fs::read_to_string(path) else { continue };
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (ts, rest) = parser::extract_timestamp(line);
                if let Some(ts) = ts {
                    last_ts = Some(ts);
                    global_min_ts = Some(global_min_ts.map_or(ts, |m: u64| m.min(ts)));
                    global_max_ts = Some(global_max_ts.map_or(ts, |m: u64| m.max(ts)));
                }
                let template = tree.classify(rest);
                if template.is_empty() {
                    continue;
                }
                raw.push((input.source_id, dict.lookup(&template), last_ts.unwrap_or(0)));
            }
        }
    }

    let base_ts = global_min_ts.unwrap_or(0);

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
