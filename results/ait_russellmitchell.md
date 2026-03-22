# AIT-LDSv2 Evaluation Results

Dataset: AIT Log Data Set V2.0 (Landauer et al., IEEE TDSC 2022)
Scenario: russellmitchell
Date: 2026-03-22

## Dataset

- Source: AIT-LDSv2 russellmitchell scenario (Zenodo 5789064)
- Simulation: 2022-01-21 00:00 to 2022-01-25 00:00 (4 days)
- Attack: 2022-01-24 03:01 to 2022-01-24 04:39 (98 min, low scan volume)
- Attack types: nmap/WPScan/dirb scans, webshell upload, password cracking, privilege escalation, data exfiltration
- 18 hosts (excluding attacker and external users)
- Log types: syslog, apache access/error, suricata JSON, audit, openvpn, dnsmasq
- 846,866 total events parsed from 71.7M lines
- 53 sources (host × log_type pairs with ≥32 events)
- Dictionary: 13,701 templates

## Approach

Day-level splits (24h windows, equal duration):
- day_0: 50 sources, 166K events (simulation startup)
- day_1: 50 sources, 178K events (normal)
- day_2: 51 sources, 191K events (normal)
- day_3: 52 sources, 311K events (contains attack)

## Results

### Day-level comparisons

| Comparison | Score | Uncertainty | Expected | Result |
|---|---|---|---|---|
| day_1 vs day_2 (normal↔normal, adjacent) | 0.14 | 0.86 | < 0.3 | ✅ |
| day_2 vs day_3 (normal↔attack, adjacent) | 1.00 | 0.00 | > 0.7 | ✅ |
| day_1 vs day_3 (normal↔attack) | 1.00 | 0.00 | > 0.7 | ✅ |
| day_0 vs day_3 (normal↔attack) | 1.00 | 0.00 | > 0.7 | ✅ |
| day_0 vs day_1 (startup↔normal) | 0.86 | 0.14 | < 0.3 | ❌ FP |
| day_0 vs day_2 (startup↔normal) | 0.83 | 0.17 | < 0.3 | ❌ FP |

### Method breakdown (best pair: day_1 vs day_2 normal, day_2 vs day_3 attack)

Normal (day_1 vs day_2):
```
dist: z_d=-1.4  z_ΔH=-0.5   (no signal)
dep:  z_d=-4.9  z_ΔH=-0.3   (no signal)
spec: z_d=-0.7  z_ΔH=-0.9   (no signal)
co:   z_d=+0.0  z_ΔH=+0.2   (no signal)
conf: z_d=-1.4  z_ΔH=+0.0   (no signal)
wav:  z_d=-4.5  z_ΔH=-0.9   (no signal)
```

Attack (day_2 vs day_3):
```
dist: z_d=+2.2  z_ΔH=+0.8   ← distributional shift detected
dep:  z_d=-1.0  z_ΔH=+8.4   ← dependency entropy shift
spec: z_d=+3.1  z_ΔH=-0.1   ← spectral divergence
co:   z_d=+0.0  z_ΔH=-1.3   (no signal)
conf: z_d=+2.2  z_ΔH=+0.0   ← evidence conflict
wav:  z_d=+2.9  z_ΔH=+5.0   ← wavelet divergence + entropy shift
```

## Key Findings

1. **Attack detection works**: All comparisons against the attack day score 1.00 with zero uncertainty. Multiple methods contribute (dist, dep, spec, conflict, wavelet).

2. **Adjacent normal days compare cleanly**: day_1 vs day_2 scores 0.14 — well below the 0.3 threshold. All z-scores are negative (no false evidence).

3. **Day 0 startup artifacts**: Comparing day_0 against later days produces false positives (0.83-0.86). Day 0 has fewer events (166K vs 178-191K) due to simulation ramp-up. This is a real distributional difference, not a pipeline bug.

4. **Attack-window-level comparisons failed**: 98-minute windows have too few active sources (10-20 with ≥32 events) for reliable multi-split baseline estimation. Day-level windows (50+ sources) work much better.

5. **Multi-method fusion adds value**: The attack is detected by 5 of 6 methods (all except co-occurrence). No single method would be sufficient — dist alone has z_d=+2.2 (moderate), but combined evidence pushes to certainty.

## Limitations

- Only one scenario evaluated (russellmitchell, low scan volume)
- Day-level granularity means the attack is diluted across 24h of normal traffic — yet still detected
- Short attack windows (< 2h) require sufficient source density for reliable comparison
- Score calibration (M10) needed to reduce day_0 false positives
