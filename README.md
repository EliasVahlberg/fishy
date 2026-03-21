# fishy

Multi-source information-fusion anomaly detection through collection comparison.

Point at two log collections — a baseline and a test — and get a verdict:

```
$ fishy -b logs/baseline/ -c logs/test/
fishy score: 0.82 — something smells off
```

## What It Does

fishy detects anomalies by comparing two multi-source log collections. It fuses information across heterogeneous sources using adaptive analysis methods — distributional divergence, cross-source dependency shift, spectral fingerprinting, and Dempster-Shafer evidence combination — weighted automatically by perceived entropy.

The user sees: fishy / not fishy.
The system does: adaptive multi-method information-fusion with self-calibrating weights.

## Workspace

- `analysis/` — Stateless analysis function library (the math)
- `fishy/` — Fusion orchestration + CLI (the product)

## Status

Early development. See `docs/` in the [design workspace](https://github.com/placeholder) for SCOPE, RESEARCH, and ARCHITECTURE documents.
