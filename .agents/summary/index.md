# Documentation Index — fishy

## How to use this index

This file is the primary context entry point for AI assistants. Each section below summarizes a documentation file and when to consult it. The summaries are sufficient to answer most questions without reading the full files.

---

## Quick reference

| Question | File |
|---|---|
| How does the fusion pipeline work? | `architecture.md` |
| What does each crate/module do? | `components.md` |
| What are the public function signatures? | `interfaces.md` |
| What do the Rust types look like? | `data_models.md` |
| How do I go from raw logs to a score? | `workflows.md` |
| What external libraries are used? | `dependencies.md` |
| What's missing or inconsistent? | `review_notes.md` |

---

## File summaries

### `architecture.md`
System-level design. Covers: crate boundary (analysis is domain-agnostic), the adaptive fusion pipeline as a flowchart, multi-baseline scoring (min-divergence against nearest baseline), and the on-disk JSON collection format. **Read this first** when making changes to the fusion logic or adding new analysis methods.

### `components.md`
Per-module responsibilities. Covers: all 8 analysis modules, fishy orchestration files, encoder files, Python scripts, and the six analysis methods with their applicability gates. **Read this** when locating where specific logic lives.

### `interfaces.md`
All public APIs. Covers: `detect()` signature, `DetectConfig` fields, `FusionStrategy` variants, CLI flags for both binaries, and the full analysis crate function list. **Read this** when calling fishy as a library or extending the CLI.

### `data_models.md`
All Rust types with field-level detail. Covers: `LogCollection`, `Event`, `AnomalyReport`, `MethodDetail`, `BPA`, and the on-disk JSON format. **Read this** when working with serialization, adding fields to reports, or understanding what `detect()` returns.

### `workflows.md`
Step-by-step processes as sequence/flowcharts. Covers: raw logs → score (encoder + fishy), multi-baseline workflow, Drain template extraction, baseline variance estimation paths (quarter-split vs pairwise), and AIT-LDSv2 preprocessing. **Read this** when tracing data flow or debugging unexpected scores.

### `dependencies.md`
External crates and datasets. Covers: per-crate dependency table, Python stdlib usage, dataset licenses, and the `parallel` feature flag. **Read this** when adding dependencies or building without rayon.

### `review_notes.md`
Gaps and inconsistencies found during documentation review. Covers: undocumented gen scenarios, applicability gate thresholds, MAX_CO_NODES rationale, unevaluated AIT scenarios, and score tier interpretation. **Read this** when something seems underdocumented.

---

## Key facts for agents

- **Entry point**: `fishy::detect(baselines: &[LogCollection], test, config)` in `fishy/src/lib.rs`
- **Crate boundary**: `analysis` never imports `fishy`. If analysis code needs a `LogCollection`, the design is wrong.
- **Single baseline**: uses quarter-split variance + sigmoid BPA (divergence only, no ΔH)
- **3+ baselines**: uses pairwise divergences + empirical CDF (both divergence and ΔH)
- **TemplateId(0)** = unknown template (not in dictionary)
- **MAX_CO_NODES = 128** in `analysis/src/co_occurrence.rs` — caps eigendecomposition cost
- **Timestamps** in collections are relative seconds from collection start
- **`prep_ait.py`** produces fishy JSON directly (bypasses encoder); attack times hardcoded for all 8 AIT-LDSv2 scenarios
- **Verdict tiers**: < 0.20 clean, < 0.40 probably fine, < 0.60 worth a look, < 0.80 smells off, ≥ 0.80 definitely fishy
