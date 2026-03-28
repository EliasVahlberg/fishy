# Review Notes

## Consistency Check

No inconsistencies found between documentation files. The architecture, components, interfaces, data models, and workflows are mutually consistent.

## Completeness Check

### Well-documented areas
- Core fusion pipeline (adaptive_inner) — fully described
- Multi-baseline path — architecture and rationale documented
- Encoder pipeline — Drain algorithm and timestamp detection documented
- Public API — all public functions documented in interfaces.md
- Data models — all types with field-level detail

### Gaps identified

1. **`gen` binary scenarios** — the 9 synthetic scenarios in `fishy/src/bin/gen.rs` are not documented in detail. Relevant when debugging calibration or adding new test scenarios.

2. **Applicability gate thresholds** — `GATE_LOW = 0.05`, `GATE_HIGH = 0.95` (normalized entropy bounds) are not explained in documentation. These determine when a method is skipped.

3. **`MAX_CO_NODES = 128`** — the co-occurrence cap is mentioned in components.md but the performance rationale (prevents O(n³) eigendecomposition on large template vocabularies) could be more explicit.

4. **`prep_ait.py` attack times** — hardcoded for all 8 AIT-LDSv2 scenarios but only russellmitchell and santos have been evaluated. The other 6 scenarios (fox, harrison, shaw, wardbeck, wheeler, wilson) are ready to run but undocumented.

5. **Score interpretation** — the verdict tiers (`< 0.20` = clean, `< 0.40` = probably fine, etc.) are in `verdict_string()` but not surfaced in user-facing docs.

## Language Support Gaps

All code is Rust + Python scripts. No gaps from language support limitations.

## Recommendations

- Add a `CALIBRATION.md` or section in README explaining score tiers and what drives each method
- Document the `gen` scenarios as a calibration reference
- Consider adding `--list-scenarios` to the gen binary
