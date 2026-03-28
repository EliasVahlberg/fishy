# AIT-LDSv2 Evaluation — santos Scenario

Dataset: AIT Log Data Set V2.0 (Landauer et al., IEEE TDSC 2022)
Scenario: santos
Date: 2026-03-28

## Dataset

- Source: AIT-LDSv2 santos scenario (Zenodo 5789064)
- Simulation: 2022-01-14 00:00 to 2022-01-18 00:00 (4 days)
- Attack: 2022-01-17 11:15 to 2022-01-17 11:59 (44 min, low scan volume)
- 23 hosts, 52 sources (host × log_type pairs with ≥32 events)
- 992,038 total events parsed from 105M lines
- Dictionary: 19,315 templates

## Results

### Single-baseline comparisons

| Comparison | Score | Uncertainty | Expected | Result |
|---|---|---|---|---|
| day_1 vs day_2 (normal↔normal, adjacent) | 0.00 | 1.00 | < 0.3 | ✅ |
| day_0 vs day_1 (normal↔normal, distant) | 0.24 | 0.76 | < 0.3 | ✅ |
| day_2 vs day_3 (normal↔attack, adjacent) | 0.82 | 0.18 | > 0.7 | ✅ |
| day_1 vs day_3 (normal↔attack) | 0.86 | 0.14 | > 0.7 | ✅ |

### Multi-baseline comparisons (empirical CDF path)

| Comparison | Score | Uncertainty | Expected | Result |
|---|---|---|---|---|
| day_0+day_1+day_2 vs day_3 (attack) | 1.00 | 0.00 | > 0.7 | ✅ |
| day_0+day_1+day_2 vs day_1 (in-distribution) | 0.00 | 1.00 | < 0.3 | ✅ |

## Key Findings

1. **No false positives**: day_0 vs day_1 scores 0.24 — well below 0.3. No startup artifacts
   visible in this scenario (unlike russellmitchell where day_0 was a FP at 0.74).

2. **Attack detected at 0.82–0.86** with single baseline despite only 44-minute attack window.
   Multi-baseline pushes to 1.00 with zero uncertainty.

3. **Replicates russellmitchell findings**: adjacent normal days score 0.00, attack day clearly
   detected. Generalizes across different attack parameters and scan volumes.

4. **Multi-baseline advantage**: 3 baselines → empirical CDF → 1.00 vs 0.82 single-baseline.
   The pairwise variance estimate correctly captures day-to-day normal variation.
