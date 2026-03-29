# fishy

Got two log collections and something smells off? fishy can tell you how off.

```
$ fishy -b logs/baseline/ -c logs/test/
fishy score: 0.82 — something smells off
```

Point it at a known-good baseline and a suspect collection. It'll tell you if they match — and if they don't, which sources and methods flagged the difference.

## Getting Started

Raw logs need tokenizing first:

```
$ encoder build-dict baseline_logs/ -o dict.json
$ encoder encode baseline_logs/ -d dict.json -o baseline/
$ encoder encode test_logs/ -d dict.json -o test/
$ fishy -b baseline/ -c test/ -v
```

The encoder uses a Drain parse tree for format-agnostic template extraction — no format flag needed. Timestamps are auto-detected (syslog, nginx, apache, ISO 8601, JSON, Unix seconds).

Already have JSON collections? Skip the encoder and point fishy at the directories directly.

## Multiple Baselines

Pass multiple `-b` flags to use the multi-baseline path. With 3+ baselines, fishy estimates the normal noise floor from pairwise divergences between baselines (empirical CDF) instead of within-collection quarter-splits. This eliminates hardcoded thresholds and reduces false positives from day-to-day drift.

```
$ fishy -b day1/ -b day2/ -b day3/ -c day4/ -v
```

## What's Going On Under the Hood

Six analysis methods look at your data from different angles — distributional divergence, cross-source dependency shifts, spectral fingerprinting, wavelet decomposition, co-occurrence structure, and evidence conflict. Each one produces a divergence score and an entropy delta. Methods that can't see anything useful in your data get automatically skipped.

With a single baseline, z-scores are computed against within-collection quarter-split variance and converted to belief assignments via a sigmoid. With 3+ baselines, the test is scored against its nearest baseline using an empirical CDF of pairwise baseline divergences — fully data-driven, no tuned parameters.

All BPAs are fused via Dempster-Shafer theory into a single score + uncertainty.

No training. No hyperparameters. Calibration comes from the baseline itself.

## Score Interpretation

| Score | Verdict |
|---|---|
| < 0.20 | looks clean |
| < 0.40 | probably fine |
| < 0.60 | worth a look |
| < 0.80 | something smells off |
| ≥ 0.80 | definitely fishy |

`uncertainty` reflects how much evidence remains uncommitted. High uncertainty means few methods fired or methods disagreed.

## Workspace

- `analysis/` — the math (stateless, domain-agnostic)
- `fishy/` — the product (orchestration + CLI)
- `encoder/` — log tokenization (raw logs → JSON collections)

## Evaluated Datasets

| Dataset | Sources | Events | Normal score | Attack score |
|---|---|---|---|---|
| BGL (supercomputer failures) | 65 | 4.4M | — | 0.96–1.00 |
| AIT-LDSv2 russellmitchell | 52 | 847K | 0.00–0.13 | 1.00 |
| AIT-LDSv2 santos | 52 | 992K | 0.00–0.24 | 0.82–1.00 |

## Research

The method is described in detail in [docs/paper/fishy_paper.pdf](docs/paper/fishy_paper.pdf):

> Elias Vahlberg. *fishy: Multi-Source Log Collection Anomaly Detection via Information-Theoretic Evidence Fusion.* March 2026.

## Status

See `ROADMAP.md` for completed and planned milestones.
