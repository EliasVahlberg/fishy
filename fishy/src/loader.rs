use crate::types::{CollectionMetadata, EventStream, LogCollection, SourceId};
use std::collections::HashMap;
use std::path::Path;

/// Load a `LogCollection` from a directory.
///
/// Expected layout: `meta.json` (CollectionMetadata) plus one
/// `<source_id>.json` (EventStream) per source.
pub fn load_collection(dir: &Path) -> Result<LogCollection, String> {
    let meta_path = dir.join("meta.json");
    let meta_bytes = std::fs::read(&meta_path)
        .map_err(|e| format!("cannot read {}: {e}", meta_path.display()))?;
    let metadata: CollectionMetadata = serde_json::from_slice(&meta_bytes)
        .map_err(|e| format!("invalid meta.json: {e}"))?;

    let mut sources = HashMap::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()) == Some("meta.json") {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("bad filename: {}", path.display()))?;
        let id: u32 = stem
            .parse()
            .map_err(|_| format!("source filename must be a u32, got '{stem}'"))?;
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let stream: EventStream =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        sources.insert(SourceId(id), stream);
    }

    if sources.is_empty() {
        return Err("collection contains no sources".into());
    }

    Ok(LogCollection { sources, metadata })
}
