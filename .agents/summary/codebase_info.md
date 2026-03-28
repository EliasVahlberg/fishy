# Codebase Info — fishy

## Identity
- **Name**: fishy
- **Language**: Rust (workspace)
- **Type**: CLI tool + library
- **Purpose**: Multi-source log collection anomaly detection via information-theoretic fusion

## Repository Structure
```
fishy/
├── analysis/       # Stateless math library (no domain types)
├── fishy/          # Orchestration + CLI binary
├── encoder/        # Log tokenization binary
├── scripts/        # Python preprocessing scripts
├── results/        # Evaluation results (markdown + JSON)
├── testdata/       # Synthetic test collections (gen binary output)
└── data/           # Raw datasets (gitignored)
```

## Technology Stack
- Rust 2021 edition, workspace with 3 crates
- `rayon` — parallel method execution (feature-flagged)
- `serde` / `serde_json` — all types Serialize+Deserialize
- `clap` (derive) — CLI parsing
- `regex` — log parsing in encoder
- Python 3 — preprocessing scripts (no runtime dependency)

## Build Profile
Release: `lto = "fat"`, `codegen-units = 1`
