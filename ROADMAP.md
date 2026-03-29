# fishy Roadmap

MVP = all six analysis methods + adaptive fusion + working CLI.

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

- [x] Baseline sub-sampling for stability estimation (multi-split: quarter splits → 3 complementary pairs)
- [x] Baseline entropy per method → applicability gate (normalized entropy thresholds)
- [x] Entropy delta (ΔH) as first-class observable per method
- [x] Z-score BPA construction: z = (observation - μ) / σ → `bpa_from_zscore` (positive z only → anomalous evidence)
- [x] Dempster-Shafer combination as primary scoring: score = m({anomalous}), uncertainty = m(Θ)
- [x] Wire all six methods into `detect()` under `FusionStrategy::Adaptive`

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

## Milestone 7 — Encoder Patch (AIT-LDSv2 Prerequisites) ✅
> Minimal encoder fixes required before real-world evaluation can run.

- [x] Apache Combined Log Format (`-f apache`) — standard Combined Log Format, distinct from nginx
- [x] Suricata JSON nested field paths (`alert.signature`, `timestamp`) — dotted path support in JSON mode
- [x] Multi-file source input — concatenate rotated log files (`auth.log`, `auth.log.1`, `auth.log.2`, …) into one source

## Milestone 8 — AIT-LDSv2 Evaluation ✅
> Run fishy against a real multi-source labeled dataset and validate scores.
> Dataset: AIT Log Data Set v2.0 (Landauer et al., IEEE TDSC 2022) — https://zenodo.org/record/5789064

- [x] Download AIT-LDSv2 scenario (russellmitchell, 7.1 GB zip, 14 GB unpacked)
- [x] Preprocessing script (`scripts/prep_ait.py`) — parse syslog, apache, suricata JSON, audit, openvpn, dnsmasq; day-level splits
- [x] Run comparison pairs (day-level windows, 50+ sources each):
  - day_1 vs day_2 (normal↔normal) → 0.14 ✅
  - day_2 vs day_3 (normal↔attack) → 1.00 ✅
  - day_1 vs day_3 (normal↔attack) → 1.00 ✅
- [x] Document results (`results/ait_russellmitchell.md`)

## Milestone 9 — Drain Encoder ✅
> Replace format-specific regex parsers with a format-agnostic Drain parse tree.
> Consistency guarantee: tree built from baseline, serialised, reused for test collection.

- [x] Drain parse tree — fixed depth, similarity threshold 0.5, digit-containing tokens → wildcard
- [x] `MaxChild` branching limit to prevent tree explosion (default 100)
- [x] Single code path replaces all `LogFormat` variants — no `-f` flag needed
- [x] Generic timestamp auto-detection (ISO 8601, syslog, nginx/apache, unix seconds, JSON)
- [x] Serialise trained tree alongside dictionary (`drain.json` next to `dict.json`)
- [x] `build-dict` builds Drain tree in first pass, dictionary in second
- [x] `encode` loads and applies serialised Drain tree

## Milestone 10 — Score Calibration ✅
> Validate that the scores mean something beyond the AIT-LDSv2 evaluation.

- [x] Synthetic collection generator (`gen` binary) — 9 severity-graded scenarios (clean, dist mild/moderate/severe, spectral mild/severe, dep_break, conflict, multi_anomaly)
- [x] Per-method sigmoid midpoints — wavelet 3.0, spec 2.5, others 2.0 (reduces day_0 FP from 0.86→0.74)
- [x] `source_weights` wired into dist divergence (weighted mean when provided)
- [x] `FusionStrategy` single-method modes — DistributionalFingerprint, DependencyShift, SpectralFingerprint, EvidenceConflict each run only their target method through adaptive pipeline

## Milestone 11 — Extended Dataset Evaluation
> Cross-validate fishy on additional labeled datasets to confirm generalization.

### AIT-LDSv2 additional scenarios (same Zenodo record, `prep_ait.py` already supports all 8)

- [x] **santos** (10 GB zip, 17 GB unpacked) — 4-day sim, low scan volume, 44-min attack. Smallest after russellmitchell.
- [ ] **fox** (15.8 GB zip, 26 GB unpacked) — 5-day sim, high scan volume, 76-min attack. Tests high-noise detection.
- [ ] **wheeler** (19.6 GB zip, 30 GB unpacked) — 5-day sim, high scan volume, 10h attack window, no password cracking. Longest attack — tests sustained anomaly detection.

### Thunderbird (CFDR supercomputer logs)

- [ ] Download Thunderbird dataset (~30 GB) from CFDR
- [ ] Preprocessing script — per-rack sources (like BGL), label-based baseline/test split
- [ ] Run fishy and record results — validates BGL findings generalize to a second supercomputer

### Success criteria

Same as M8:
- baseline vs baseline → score < 0.3
- normal vs normal (adjacent days) → score < 0.3
- normal vs attack → score > 0.7

## Milestone 12 — Multi-Baseline Support ✅
> Replace within-collection quarter-split variance with real between-collection variance.
> Enables self-calibrating thresholds and eliminates hardcoded per-method sigmoid midpoints.

- [x] `detect(baselines: &[LogCollection], test, config)` — API change, single baseline still works
- [x] Pairwise baseline divergences → empirical CDF per method (replaces sigmoid + METHOD_MIDPOINTS)
- [x] Min-divergence scoring: test compared against nearest baseline, not fixed reference
- [x] Baseline outlier rejection — flag baselines with mean pairwise divergence >2σ from group
- [x] Trend detection — linear regression on ordered baselines, flag if test breaks trend
- [x] CLI: multiple `-b` flags accepted
- [x] Validated on AIT russellmitchell:
  - Single baseline day_1 vs day_2 (normal): 0.00 ✅
  - Single baseline day_2 vs day_3 (attack): 0.96 ✅
  - 3 baselines day_0+day_1+day_2 vs day_3 (attack): 1.00 ✅
  - 3 baselines vs day_1 (in-distribution): 0.00 ✅
  - 3 baselines vs day_2 (in-distribution): 0.00 ✅

## Milestone 13 — Performance Optimization
> Profile and fix the main CPU bottlenecks before considering GPU acceleration.

### Known issues (fix first)
- [ ] Cache extracted `Representations` — `reject_outlier_baselines` and `pairwise_baseline_stats` both call `extract()` independently, causing redundant eigendecompositions. Pre-extract all baselines once and reuse.
- [ ] Deduplicate pairwise extractions — in `pairwise_baseline_stats`, each baseline is extracted once per pair it appears in. With 3 baselines, baseline[0] is extracted twice. Cache by index.

### Profiling
- [ ] Profile a 3-baseline run with `cargo flamegraph` to confirm eigendecomposition dominates
- [ ] Measure per-method wall time to identify secondary bottlenecks

### Potential further optimizations (after profiling)
- [ ] Incremental eigendecomposition — warm-start from previous result when collections are similar
- [ ] Skip co-occurrence for sources with low event counts (already gated at 32, consider raising)

### GPU acceleration (future exploration)
> Add only after CPU optimizations are exhausted and profiling shows GPU would help.
- [ ] `wgpu` feature flag — optional GPU compute path, falls back to CPU
- [ ] Batched FFT across all sources × all baseline pairs (most parallelizable, well-understood)
- [ ] Batched eigendecomposition (128×128 Jacobi iteration in WGSL) — hardest part, highest potential speedup
- [ ] Benchmark: measure actual speedup on realistic workload (3 baselines, 50+ sources)
