# Dependencies — fishy

## Rust dependencies

### fishy crate
| Crate | Version | Usage |
|---|---|---|
| `analysis` | path | Internal math library |
| `clap` | 4 (derive) | CLI argument parsing |
| `serde` + `serde_json` | 1 | Serialize/Deserialize all types, JSON I/O |
| `rayon` | 1 (optional) | Parallel method execution, feature `parallel` (default on) |

### encoder crate
| Crate | Version | Usage |
|---|---|---|
| `analysis` | path | `SourceId`, `TemplateId` types |
| `fishy` | path | `LogCollection`, `Event`, `EventStream` types |
| `clap` | 4 (derive) | CLI |
| `serde` + `serde_json` | 1 | DrainTree + Dictionary serialization |
| `regex` | 1 | Timestamp pattern matching in parser.rs |

### analysis crate
| Crate | Version | Usage |
|---|---|---|
| `serde` | 1 | Derive on all types |

## Python dependencies (scripts only)

No runtime Python dependency for fishy itself. Scripts use only stdlib:
- `json`, `os`, `pathlib`, `datetime`, `re`, `collections` — all stdlib

## Dataset sources

| Dataset | Source | License |
|---|---|---|
| AIT-LDSv2 | Zenodo 5789064 | CC BY-NC-SA 4.0 |
| BGL | LogHub (Zenodo 3227177) | — |

## Feature flags

```toml
[features]
default = ["parallel"]
parallel = ["dep:rayon"]
```

Build without parallelism: `cargo build --no-default-features`
