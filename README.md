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

## What's Going On Under the Hood

Six analysis methods look at your data from different angles — distributional divergence, cross-source dependency shifts, spectral fingerprinting, wavelet decomposition, co-occurrence structure, and evidence conflict. Each one produces a divergence score and an entropy delta. Methods that can't see anything useful in your data get automatically skipped. The rest have their signals converted to z-scores against baseline noise, turned into belief assignments, and fused via Dempster-Shafer theory into a single score + uncertainty.

No training. No hyperparameters. Calibration comes from the baseline itself.

## Workspace

- `analysis/` — the math (stateless, domain-agnostic)
- `fishy/` — the product (orchestration + CLI)
- `encoder/` — log tokenization (raw logs → JSON collections)

## Status

Early development. See `ROADMAP.md` for what's done and what's next.
