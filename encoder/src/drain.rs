use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const WILDCARD: &str = "<*>";

/// Fixed-depth parse tree for format-agnostic log template extraction (Drain algorithm).
///
/// Build from baseline logs with [`train`], serialize, then reuse for test collection
/// via [`classify`] to guarantee template consistency across collections.
#[derive(Debug, Serialize, Deserialize)]
pub struct DrainTree {
    sim_threshold: f64,
    max_children: usize,
    /// length → first-token → log groups
    root: HashMap<usize, HashMap<String, Vec<LogGroup>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogGroup {
    template: Vec<String>, // constant tokens or "<*>"
    count: u64,
}

impl DrainTree {
    pub fn new(sim_threshold: f64, max_children: usize) -> Self {
        Self { sim_threshold, max_children, root: HashMap::new() }
    }

    /// Insert a line into the tree, updating or creating a log group.
    /// Returns the template string for this line's cluster.
    pub fn train(&mut self, line: &str) -> String {
        let tokens = tokenize(line);
        if tokens.is_empty() {
            return String::new();
        }
        let len = tokens.len();
        let key = first_token_key(&tokens);

        let groups = self
            .root
            .entry(len)
            .or_default()
            .entry(key.clone())
            .or_default();

        // Find best matching group
        if let Some(idx) = best_match(groups, &tokens, self.sim_threshold) {
            // Merge: replace differing tokens with wildcard
            let g = &mut groups[idx];
            for (i, tok) in tokens.iter().enumerate() {
                if g.template[i] != *tok && g.template[i] != WILDCARD {
                    g.template[i] = WILDCARD.to_string();
                }
            }
            g.count += 1;
            return template_string(&groups[idx].template);
        }

        // No match — create new group (if under MaxChild limit)
        if groups.len() < self.max_children {
            let template: Vec<String> = tokens
                .iter()
                .map(|t| if has_digit(t) { WILDCARD.to_string() } else { t.clone() })
                .collect();
            let s = template_string(&template);
            groups.push(LogGroup { template, count: 1 });
            return s;
        }

        // Over limit — merge into the most similar existing group
        let idx = most_similar(groups, &tokens);
        let g = &mut groups[idx];
        for (i, tok) in tokens.iter().enumerate() {
            if g.template[i] != *tok && g.template[i] != WILDCARD {
                g.template[i] = WILDCARD.to_string();
            }
        }
        g.count += 1;
        template_string(&groups[idx].template)
    }

    /// Classify a line against the trained tree without modifying it.
    /// Returns the best matching template, or a wildcard-normalized fallback.
    pub fn classify(&self, line: &str) -> String {
        let tokens = tokenize(line);
        if tokens.is_empty() {
            return String::new();
        }
        let len = tokens.len();
        let key = first_token_key(&tokens);

        if let Some(by_key) = self.root.get(&len) {
            // Try exact first-token match, then wildcard bucket
            for k in [&key, &WILDCARD.to_string()] {
                if let Some(groups) = by_key.get(k.as_str()) {
                    if let Some(idx) = best_match_readonly(groups, &tokens, self.sim_threshold) {
                        return template_string(&groups[idx].template);
                    }
                }
            }
        }

        // No match — return wildcard-normalized tokens (unknown template)
        tokens
            .iter()
            .map(|t| if has_digit(t) { WILDCARD } else { t.as_str() })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let s = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }

    pub fn num_clusters(&self) -> usize {
        self.root.values().flat_map(|m| m.values()).map(|g| g.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tokenize(line: &str) -> Vec<String> {
    line.split_whitespace().map(String::from).collect()
}

fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

fn first_token_key(tokens: &[String]) -> String {
    if has_digit(&tokens[0]) {
        WILDCARD.to_string()
    } else {
        tokens[0].clone()
    }
}

fn similarity(template: &[String], tokens: &[String]) -> f64 {
    if template.len() != tokens.len() {
        return 0.0;
    }
    let matches = template
        .iter()
        .zip(tokens.iter())
        .filter(|(t, tok)| *t == WILDCARD || *t == *tok)
        .count();
    matches as f64 / template.len() as f64
}

fn best_match(groups: &[LogGroup], tokens: &[String], threshold: f64) -> Option<usize> {
    let mut best_idx = None;
    let mut best_sim = threshold;
    for (i, g) in groups.iter().enumerate() {
        let sim = similarity(&g.template, tokens);
        if sim >= best_sim {
            best_sim = sim;
            best_idx = Some(i);
        }
    }
    best_idx
}

fn best_match_readonly(groups: &[LogGroup], tokens: &[String], threshold: f64) -> Option<usize> {
    best_match(groups, tokens, threshold)
}

fn most_similar(groups: &[LogGroup], tokens: &[String]) -> usize {
    groups
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            similarity(&a.template, tokens)
                .partial_cmp(&similarity(&b.template, tokens))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn template_string(template: &[String]) -> String {
    template.join(" ")
}
