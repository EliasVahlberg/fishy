# AGENTS.md — fishy

## Workspace layout

```
analysis/       stateless math library — no domain types, no I/O
fishy/          orchestration + CLI binary (depends on analysis)
encoder/        log tokenization binary (depends on fishy for types)
scripts/        Python preprocessing (prep_ait.py, prep_bgl.py)
results/        evaluation results (markdown + JSON, committed)
testdata/       synthetic collections from `cargo run --bin gen`
data/           raw datasets (gitignored, large)
```

## Key entry points

| What | Where |
|---|---|
| Fusion pipeline | `fishy/src/lib.rs` → `detect()` → `adaptive_inner()` |
| Representation extraction | `fishy/src/extract.rs` → `extract()` |
| Drain template clustering | `encoder/src/drain.rs` → `DrainTree::train()` / `classify()` |
| Timestamp auto-detection | `encoder/src/parser.rs` → `extract_timestamp()` |
| Synthetic test data | `fishy/src/bin/gen.rs` → `cargo run --bin gen` |
| AIT-LDSv2 preprocessing | `scripts/prep_ait.py` |

## Crate boundary rule

`analysis` must never import `fishy` types. It operates on `&[f64]`, `EventDistribution`, `MIMatrix`, etc. If an analysis function needs a `LogCollection`, the design is wrong — extract the representation in `fishy/src/extract.rs` first.

## Patterns that deviate from defaults

**Multi-baseline API**: `detect()` takes `baselines: &[LogCollection]` (a slice), not a single baseline. Single baseline still works (slice of length 1) but uses a different variance estimation path (quarter-split + sigmoid). Three or more baselines use pairwise divergences + empirical CDF.

**Empirical CDF uses strict inequality**: `count(samples < value)` not `≤`. This prevents a test value equal to a pairwise sample from scoring 1.0.

**ΔH BPAs suppressed in single-baseline mode**: Within-collection entropy variance underestimates between-collection variance. ΔH BPAs are only produced in the empirical CDF path (≥3 baselines).

**Min-divergence scoring**: With multiple baselines, the test is scored against its nearest baseline (minimum divergence per method), not a fixed reference. This prevents startup-day drift from inflating scores.

**Co-occurrence cap**: `MAX_CO_NODES = 128` in `analysis/src/co_occurrence.rs`. Without this, eigendecomposition on large template vocabularies is O(n³) and takes hours.

**Encoder saves two files**: `build-dict` writes both `dict.json` and `drain.json` to the same directory. `encode` loads both automatically from the dict path — no separate flag needed.

## Repo-specific tools

```bash
cargo run --bin gen                          # generate testdata/ synthetic scenarios
cargo run --bin fishy -- -b B/ -c T/ -v     # run comparison with verbose output
cargo run --bin encoder -- build-dict DIR/  # train Drain tree + build dictionary
python3 scripts/prep_ait.py data/ait/<scenario>  # preprocess AIT-LDSv2 scenario
```

## Score interpretation

| Score | Verdict |
|---|---|
| < 0.20 | looks clean |
| < 0.40 | probably fine |
| < 0.60 | worth a look |
| < 0.80 | something smells off |
| ≥ 0.80 | definitely fishy |

`uncertainty` = m(Θ) from DS combination. High uncertainty means methods disagree or few methods fired.

## Custom Instructions
<!-- This section is for human and agent-maintained operational knowledge.
     Add repo-specific conventions, gotchas, and workflow rules here.
     This section is preserved exactly as-is when re-running codebase-summary. -->
