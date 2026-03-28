# Interfaces — fishy

## fishy library public API

### Entry point
```rust
pub fn detect(
    baselines: &[LogCollection],
    test: &LogCollection,
    config: &DetectConfig,
) -> Result<AnomalyReport, DetectError>
```
Single baseline: pass a slice of length 1 (sigmoid fallback path).
Three or more baselines: empirical CDF path (recommended).

### Config
```rust
pub struct DetectConfig {
    pub mode: ComparisonMode,           // SingleOrigin | MultiOrigin
    pub strategy: FusionStrategy,       // Adaptive (default) | single-method modes
    pub source_weights: Option<HashMap<SourceId, f32>>,  // weighted dist divergence
    pub significance_threshold: f32,    // verdict threshold (default 0.5)
    pub duration_tolerance: f32,        // temporal validation (0.0 = disabled)
}
```
`DetectConfig::default()` → SO mode, Adaptive strategy, threshold 0.5, tolerance 0.5.

### FusionStrategy variants
- `Adaptive` — all applicable methods fused
- `DistributionalFingerprint` — dist only
- `DependencyShift` — dep only
- `SpectralFingerprint` — spec only
- `EvidenceConflict` — conflict only

### Loader
```rust
pub fn load_collection(path: &Path) -> Result<LogCollection, String>
```

## CLI interfaces

### fishy binary
```
fishy -b <baseline_dir> [-b <baseline_dir> ...] -c <test_dir> [options]

Options:
  -b, --baseline <DIR>          Baseline collection (repeat for multi-baseline)
  -c, --compare <DIR>           Test collection
  -m, --mode <so|mo>            Comparison mode (default: so)
  -t, --threshold <f32>         Anomaly threshold (default: 0.5)
  -v, --verbose                 Per-source + per-method breakdown
      --json                    Full AnomalyReport as JSON
      --duration-tolerance <f32> Temporal validation tolerance (default: 0.5, 0.0=off)
```
Exit code: 0 = clean, 1 = anomalous, 2 = error.

### encoder binary
```
encoder build-dict <dir> [-o dict.json] [--sim-threshold 0.5] [--max-children 100]
encoder encode <dir> -d dict.json -o <output_dir>
```
`build-dict` writes both `dict.json` and `drain.json` (same directory).
`encode` loads both automatically from the dict path.

## analysis crate public API

All functions are pure (no side effects, no global state).

```rust
// Distributional
distributional_divergence(baseline: &EventDistribution, test: &EventDistribution) -> f64

// Dependency
mutual_information_matrix_timed(collection: &[(SourceId, &[(TemplateId, u64)])], bin_width: u64) -> MIMatrix
mi_matrix_divergence(baseline: &MIMatrix, test: &MIMatrix) -> f64

// Spectral
spectral_fingerprint(event_times: &[u64], bin_width: u64) -> PowerSpectrum
spectral_divergence(baseline: &PowerSpectrum, test: &PowerSpectrum) -> f64
wavelet_decompose(event_times: &[u64], bin_width: u64, levels: usize) -> WaveletCoefficients

// Co-occurrence
co_occurrence_spectrum(events: &[(TemplateId, u64)], window: u64) -> EigenSpectrum

// Dempster-Shafer
ds_combine(a: &BPA, b: &BPA) -> BPA
ds_combine_many(bpas: &[BPA]) -> BPA
ds_conflict(a: &BPA, b: &BPA) -> f64

// Evidence
bpa_from_zscore(z: f64, mapping: &BpaMapping) -> BPA
evidence_bpa(divergence: f64, confidence: f64) -> BPA

// Entropy
shannon_entropy(distribution: &[f64]) -> f64
spectral_entropy(spectrum: &PowerSpectrum) -> f64
matrix_entropy(matrix: &MIMatrix) -> f64
```
