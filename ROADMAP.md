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
