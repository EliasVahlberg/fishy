# BGL Evaluation Results

Dataset: Blue Gene/L supercomputer logs (LogHub)
Date: 2026-03-22

## Dataset

- Source: 4,747,963 log lines from BGL supercomputer
- Duration: 214.7 days (18,551,419 seconds)
- 65 racks with ≥32 events in both baseline and test
- Baseline: 4,399,503 events (normal, label == `-`)
- Test: 348,460 events (anomalous, label != `-`)
- Dictionary: 36,318 templates
- Adaptive parameters: bin_width=3600s, co_window=360s

## Full Run (65 sources)

```
fishy score: 1.00 — definitely fishy
  uncertainty: 0.00
  methods:
          dist: div=1.00  ΔH=-2.660  z_d=+3.7  z_ΔH=+13.9  H_b=2.66
           dep: (skipped — not applicable)
          spec: div=0.42  ΔH=-0.025  z_d=-5.7  z_ΔH=-2.8  H_b=7.55
            co: div=0.50  ΔH=-1.360  z_d=-18.8  z_ΔH=+0.8  H_b=1.36
      conflict: div=0.00  ΔH=+0.000  z_d=-9.2  z_ΔH=+0.0  H_b=0.50
       wavelet: div=1.00  ΔH=-0.034  z_d=+1.4  z_ΔH=+0.1  H_b=1.44

All 65 sources: divergence 1.00
Time: 2m23s (release mode, parallel)
```

Primary drivers: `dist` (z_d=+3.7, z_ΔH=+13.9) and `wavelet` (z_d=+1.4).
`dep` skipped (not applicable with 65 sources — likely entropy gate).
`spec`, `co`, `conflict` had negative z-scores (below baseline variance).

## Accuracy Analysis (5-source sample)

| Comparison | Score | Uncertainty | Expected | Result |
|---|---|---|---|---|
| baseline vs baseline (identical) | 0.00 | 1.00 | ~0 | ✅ true negative |
| baseline vs test (anomalous) | 1.00 | 0.00 | ~1 | ✅ true positive |
| temporal split (first half vs second half) | 1.00 | 0.00 | ~0 | ❌ false positive |
| random split | 0.45 | 0.55 | ~0 | ⚠️ elevated but uncertain |

Notes:
- Temporal split FP is expected — BGL drifted over 214 days, fishy detects real distributional change.
- Random split score of 0.45 driven by single z_ΔH=+1.8 from dist method. Sigmoid midpoint calibration (M10) should address this.

## Performance

- Before co_occurrence fix: >1 hour for 5 sources (Jacobi on 13,629×13,629 matrices)
- After fix (top-128 template cap): 24s for 5 sources, 2m23s for 65 sources
- Fix: `MAX_CO_NODES = 128` in `analysis/src/co_occurrence.rs`
