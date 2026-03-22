# fishy Roadmap

MVP = all five analysis methods + adaptive fusion + working CLI.

## Milestone 1 — Analysis Primitives ✅
> The math everything else depends on.

- [x] `shannon_entropy`
- [x] `ds_combine`, `ds_combine_many`, `ds_conflict`
- [x] `evidence_bpa`

## Milestone 2 — First Method + Working CLI ✅
> One complete path from disk to verdict. Validates the architecture end-to-end.

- [x] Define on-disk input format (one JSON file per source: `<source_id>.json`)
- [x] `distributional_divergence` (JSD)
- [x] `detect()` — distributional-only path: temporal validation → extraction → JSD → `evidence_bpa` → score → verdict
- [x] CLI: load collections from directory, call `detect()`, print output

After this milestone: `fishy -b baseline/ -c test/` works.

## Milestone 3 — Remaining Analysis Methods ✅
> Fill in the other four lenses. Each is independent.

- [x] `mutual_information_matrix` + `mi_matrix_divergence`
- [x] `spectral_fingerprint` + `spectral_divergence` + `spectral_entropy`
- [x] `wavelet_decompose`
- [x] `co_occurrence_spectrum` + eigenvalue computation
- [x] `matrix_entropy`

## Milestone 4 — Adaptive Fusion ✅
> The core contribution.

- [x] Baseline sub-sampling for stability estimation
- [x] Perceived entropy per method
- [x] Weight computation: `entropy × 1/(1 + variance)`
- [x] Weighted mean score across methods (DS combination used for `meta_conflict` only)
- [x] Meta-conflict mass as a reported signal
- [x] Wire all five methods into `detect()` under `FusionStrategy::Adaptive`

After this milestone: fishy is the actual product.

## Milestone 5 — Polish ✅

- [x] `--verbose` per-source breakdown
- [x] `--json` full `AnomalyReport` output
- [x] Multi-origin mode (overlapping sources only)
- [x] Rayon parallelism behind `parallel` feature flag
- [x] Applicability guards (skip dependency shift if <2 sources, skip spectral/co-occurrence if <32 events)

## Milestone 6 — Encoder ✅
> Tokenise raw log files into fishy's on-disk format.

- [x] `Dictionary` — frequency-ranked template IDs (most frequent → `TemplateId(1)`, `TemplateId(0)` reserved for unknown)
- [x] `LogFormat` — `NginxAccess`, `Syslog`, `Json`, `Custom`, `Bgl`
- [x] Sticky timestamps — events inherit the last seen timestamp until a new one appears
- [x] `build-dict` CLI subcommand — two-pass frequency count → rank
- [x] `encode` CLI subcommand — writes `<source_id>.json` + `meta.json` per collection
- [x] BGL dataset support — label-based baseline/test split, per-rack sources

## Milestone 7 — Drain Encoder
> Replace format-specific regex parsers with a format-agnostic Drain parse tree.
> Consistency guarantee: tree built from baseline, serialised, reused for test collection.

- [ ] Drain parse tree — fixed depth (3–4), token similarity threshold ~0.5, digit-containing tokens route to wildcard
- [ ] `MaxChild` branching limit to prevent tree explosion
- [ ] Single code path replaces all `LogFormat` variants
- [ ] Serialise trained tree alongside dictionary (`drain.json` next to `dict.json`)
- [ ] `build-dict` updated to build Drain tree in first pass, dictionary in second
- [ ] `encode` updated to load and apply serialised Drain tree

## Milestone 8 — Evaluation Framework
> Validate that the scores mean something.

- [ ] Synthetic collection generator (`gen` binary) — inject controlled anomalies (rate shift, template swap, dependency break, spectral shift)
- [ ] Score calibration — establish expected score ranges per anomaly type and severity
- [ ] `top_events` population — per-source event attribution in `AnomalyReport`
- [ ] `source_weights` actually used in scoring (currently unused field)
- [ ] `FusionStrategy` stubs implemented (`Distributional`, `Spectral`, `Dependency`)
- [ ] Conflict stability estimation (currently hardcoded `0.0`)

## Milestone 9 — Real-World Validation
> Run against public datasets and record results.

- [ ] Loghub BGL full dataset — multi-source baseline vs. anomalous window, record per-method scores
- [ ] Loghub HDFS — distributed system, multi-source dependency shift expected
- [ ] Document score ranges and verdict calibration against known anomaly labels
