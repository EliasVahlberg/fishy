# Data Models — fishy

## Core domain types (fishy crate)

```mermaid
classDiagram
    class LogCollection {
        +sources: HashMap~SourceId, EventStream~
        +metadata: CollectionMetadata
    }
    class CollectionMetadata {
        +start_time: u64
        +end_time: u64
    }
    class EventStream {
        +events: Vec~Event~
    }
    class Event {
        +template_id: TemplateId
        +timestamp: Option~u64~
        +params: HashMap~String, String~
    }
    LogCollection --> CollectionMetadata
    LogCollection --> EventStream
    EventStream --> Event
```

## Analysis types (analysis crate)

```mermaid
classDiagram
    class SourceId {
        +0: u32
    }
    class TemplateId {
        +0: u32
    }
    class BPA {
        +normal: f64
        +anomalous: f64
        +uncertain: f64
    }
    class EventDistribution {
        +counts: HashMap~TemplateId, u64~
        +total: u64
    }
    class MIMatrix {
        +sources: Vec~SourceId~
        +values: Vec~Vec~f64~~
    }
    class PowerSpectrum {
        +frequencies: Vec~f64~
        +magnitudes: Vec~f64~
    }
    class WaveletCoefficients {
        +levels: Vec~Vec~f64~~
    }
    class EigenSpectrum {
        +eigenvalues: Vec~f64~
    }
    class BpaMapping {
        <<enumeration>>
        Sigmoid
        Proportional
    }
```

`SourceId(0)` and `TemplateId(0)` are reserved: `TemplateId(0)` = unknown template, `SourceId` values are assigned by sorted source name order.

## Output types

```mermaid
classDiagram
    class AnomalyReport {
        +score: f64
        +uncertainty: f64
        +verdict: String
        +source_scores: HashMap~SourceId, SourceReport~
        +pair_scores: Vec~PairReport~
        +missing_sources: MissingSourceReport
        +meta_conflict: f64
        +methods: Vec~MethodDetail~
        +baseline_count: usize
        +rejected_baselines: Vec~usize~
    }
    class MethodDetail {
        +name: String
        +applicable: bool
        +divergence: f64
        +entropy_delta: f64
        +baseline_entropy: f64
        +divergence_percentile: Option~f64~
        +entropy_delta_percentile: Option~f64~
        +z_divergence: f64
        +z_entropy_delta: f64
        +trend_z: Option~f64~
    }
    class SourceReport {
        +divergence: f64
        +contribution: f64
        +top_events: Vec~TemplateId, f64~
    }
    AnomalyReport --> MethodDetail
    AnomalyReport --> SourceReport
```

## On-disk JSON format

**meta.json**
```json
{"start_time": 1642118400, "end_time": 1642204800}
```

**`<source_id>.json`**
```json
{
  "events": [
    {"template_id": 42, "timestamp": 3600, "params": {}},
    {"template_id": 1,  "timestamp": 7200, "params": {}}
  ]
}
```

Timestamps are relative seconds from collection start (set by encoder) or absolute Unix seconds (set by prep scripts). fishy uses relative timestamps internally after loading.
