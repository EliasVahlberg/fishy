# Workflows — fishy

## End-to-end: raw logs → anomaly score

```mermaid
sequenceDiagram
    participant User
    participant encoder
    participant fishy

    User->>encoder: build-dict baseline_logs/ -o dict.json
    encoder->>encoder: Train DrainTree on all lines
    encoder->>encoder: Classify lines → template frequencies
    encoder-->>User: dict.json + drain.json

    User->>encoder: encode baseline_logs/ -d dict.json -o baseline/
    encoder->>encoder: Load drain.json + dict.json
    encoder->>encoder: Classify each line → (template_id, timestamp)
    encoder-->>User: baseline/ collection directory

    User->>encoder: encode test_logs/ -d dict.json -o test/
    encoder-->>User: test/ collection directory

    User->>fishy: fishy -b baseline/ -c test/ -v
    fishy->>fishy: load_collection() × 2
    fishy->>fishy: extract() → Representations
    fishy->>fishy: adaptive_inner() → AnomalyReport
    fishy-->>User: score + verdict + per-method breakdown
```

## Multi-baseline workflow (recommended)

```mermaid
sequenceDiagram
    participant User
    participant fishy

    User->>fishy: fishy -b day1/ -b day2/ -b day3/ -c day4/
    fishy->>fishy: reject_outlier_baselines() — flag >2σ outliers
    fishy->>fishy: pairwise_baseline_stats() — N×N-1/2 divergence samples
    fishy->>fishy: For each baseline: compute test divergence
    fishy->>fishy: min-divergence per method (nearest baseline)
    fishy->>fishy: empirical_commitment() — strict CDF percentile
    fishy->>fishy: DS combination of all BPAs
    fishy-->>User: AnomalyReport with divergence_percentile per method
```

## Encoder: Drain template extraction

```mermaid
flowchart LR
    Line["raw log line"] --> TS["extract_timestamp()\nauto-detect format"]
    TS --> Rest["message remainder"]
    Rest --> Tokenize["split on whitespace"]
    Tokenize --> Key["first_token_key()\ndigit tokens → wildcard"]
    Key --> Tree["DrainTree lookup\nlength → first_token → groups"]
    Tree --> Sim{"similarity ≥ 0.5?"}
    Sim -- yes --> Merge["merge: replace\ndiffering tokens with wildcard"]
    Sim -- no --> New["new group\n(if under MaxChild limit)"]
    Merge --> Template["template string"]
    New --> Template
```

## Baseline variance estimation paths

```mermaid
flowchart TD
    N{Number of\nbaselines}
    N -- 1 or 2 --> QS["Quarter-split of first baseline\n3 within-collection samples\n→ sigmoid BPA mapping\n(divergence only, no ΔH)"]
    N -- 3+ --> PW["Pairwise divergences\nN×N-1/2 between-collection samples\n→ empirical CDF commitment\n(divergence + ΔH both used)"]
```

The ΔH (entropy delta) signal is suppressed in the sigmoid fallback because within-collection entropy variance severely underestimates between-collection entropy variance, causing false positives.

## AIT-LDSv2 preprocessing (prep_ait.py)

```mermaid
flowchart LR
    Raw["gather/<host>/logs/\nsyslog · apache · suricata\naudit · openvpn · dnsmasq"] --> Parse["format-specific parsers\n→ (timestamp, message)"]
    Parse --> Norm["norm() — strip IPs, hex,\nnumbers, paths → token"]
    Norm --> GlobalID["global_id_map\nsorted source keys → SourceId"]
    GlobalID --> DaySplit["day-level splits\n86400s windows"]
    DaySplit --> Collections["collections/day_0 … day_N\nfishy JSON format"]
```
