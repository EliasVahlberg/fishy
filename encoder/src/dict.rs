use analysis::TemplateId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Maps template strings ↔ stable `TemplateId` integers.
///
/// `TemplateId(0)` is reserved for unknown patterns (lines not seen during `build_dictionary`).
#[derive(Debug, Serialize, Deserialize)]
pub struct Dictionary {
    /// template string → id
    map: HashMap<String, u32>,
    /// id → template string (for human-readable output)
    templates: Vec<String>,
}

impl Dictionary {
    pub fn new() -> Self {
        // Reserve id 0 for unknown.
        Self { map: HashMap::new(), templates: vec!["<unknown>".into()] }
    }

    /// Intern a template string, returning its `TemplateId`. Idempotent.
    pub fn intern(&mut self, template: String) -> TemplateId {
        if let Some(&id) = self.map.get(&template) {
            return TemplateId(id);
        }
        let id = self.templates.len() as u32;
        self.map.insert(template.clone(), id);
        self.templates.push(template);
        TemplateId(id)
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
        self.templates.len() - 1 // exclude the reserved unknown slot
    }
}
