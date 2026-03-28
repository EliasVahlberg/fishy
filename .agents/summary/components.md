# Components — fishy

## analysis crate (`analysis/src/`)

Stateless math. No domain types, no I/O. Each module is independent.

| Module | Responsibility |
|---|---|
| `distributional.rs` | JSD between template frequency distributions |
| `dependency.rs` | Mutual information matrix + Frobenius divergence |
| `spectral.rs` | FFT power spectrum + DWT wavelet decomposition + divergences |
| `co_occurrence.rs` | Laplacian eigenvalue spectrum of event co-occurrence graph. Capped at `MAX_CO_NODES=128` to bound eigendecomposition cost. |
| `ds.rs` | Dempster-Shafer combination and conflict |
| `evidence.rs` | `bpa_from_zscore` (sigmoid mapping), `evidence_bpa` |
| `entropy.rs` | Shannon, spectral flatness, matrix entropy |
| `types.rs` | `SourceId`, `TemplateId`, `BPA`, `EventDistribution`, `MIMatrix`, `PowerSpectrum`, `WaveletCoefficients`, `EigenSpectrum`, `BpaMapping` |

## fishy crate (`fishy/src/`)

Orchestration layer. Owns domain types and the fusion pipeline.

| File | Responsibility |
|---|---|
| `types.rs` | `LogCollection`, `Event`, `EventStream`, `DetectConfig`, `AnomalyReport`, `MethodDetail`, `FusionStrategy`, `ComparisonMode` |
| `lib.rs` | `detect()` entry point, full adaptive fusion pipeline, multi-baseline stats, outlier rejection, trend detection |
| `extract.rs` | `extract()` — converts `LogCollection` → `Representations` (all 5 method inputs) |
| `loader.rs` | `load_collection()` — reads a directory of JSON files into `LogCollection` |
| `main.rs` | CLI (`-b` × N, `-c`, `--verbose`, `--json`, `--duration-tolerance`) |
| `bin/gen.rs` | Synthetic collection generator — 9 severity-graded scenarios |

## encoder crate (`encoder/src/`)

Log tokenization. Converts raw log files into fishy's JSON format.

| File | Responsibility |
|---|---|
| `drain.rs` | `DrainTree` — format-agnostic template clustering. Fixed-depth tree (length → first_token → groups), similarity threshold 0.5, `MaxChild=100`. |
| `parser.rs` | `extract_timestamp()` — auto-detects ISO 8601, syslog, nginx/apache, unix seconds, JSON timestamp fields |
| `dict.rs` | `Dictionary` — frequency-ranked `TemplateId` map. Most frequent template → `TemplateId(1)`, unknown → `TemplateId(0)`. |
| `lib.rs` | `build_drain_tree()`, `build_dictionary()`, `encode()` |
| `main.rs` | CLI: `build-dict` (writes `dict.json` + `drain.json`), `encode` (loads both, writes collection) |

## scripts/

Python preprocessing for specific datasets. Not part of the Rust build.

| Script | Purpose |
|---|---|
| `prep_ait.py` | AIT-LDSv2 scenarios → day-level fishy collections. Handles syslog, apache, suricata JSON, audit, openvpn, dnsmasq. Attack times hardcoded for all 8 scenarios. |
| `prep_bgl.py` | BGL supercomputer logs → per-rack fishy collections with label-based baseline/test split. |

## Six Analysis Methods

| Index | Name | Signal | Applicability gate |
|---|---|---|---|
| 0 | dist | JSD between template distributions | Always applicable |
| 1 | dep | MI matrix Frobenius divergence | ≥2 sources |
| 2 | spec | FFT power spectrum divergence | ≥32 events in any source |
| 3 | co | Laplacian eigenvalue JSD | ≥32 events in any source |
| 4 | conflict | DS conflict between per-source BPAs | ≥2 sources |
| 5 | wavelet | DWT energy level divergence | ≥32 events in any source |
