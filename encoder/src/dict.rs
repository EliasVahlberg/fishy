use analysis::TemplateId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Maps template strings ↔ frequency-ranked `TemplateId` integers.
///
/// IDs are assigned by descending frequency: most common template → `TemplateId(1)`.
/// `TemplateId(0)` is reserved for unknown patterns (not seen during `build_dictionary`).
#[derive(Debug, Serialize, Deserialize)]
pub struct Dictionary {
    map: HashMap<String, u32>,
    templates: Vec<String>, // index = id; [0] = "<unknown>"
}

impl Dictionary {
    fn empty() -> Self {
        Self { map: HashMap::new(), templates: vec!["<unknown>".into()] }
    }

    /// Build a frequency-ranked dictionary from `(template, count)` pairs.
    pub fn from_frequencies(mut freqs: Vec<(String, u64)>) -> Self {
        freqs.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let mut d = Self::empty();
        for (template, _) in freqs {
            let id = d.templates.len() as u32;
            d.map.insert(template.clone(), id);
            d.templates.push(template);
        }
        d
    }

    /// Look up a template string. Returns `TemplateId(0)` if not found.
    pub fn lookup(&self, template: &str) -> TemplateId {
        TemplateId(*self.map.get(template).unwrap_or(&0))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }

    pub fn len(&self) -> usize {
        self.templates.len() - 1
    }
}
