# fishy: Multi-Source Log Collection Anomaly Detection via Information-Theoretic Evidence Fusion

**Elias Vahlberg**
*March 2026*

---

## Abstract

We present fishy, a system for detecting anomalies in multi-source log collections through information-theoretic evidence fusion. Unlike existing log anomaly detection methods that classify individual events or sessions within a single ongoing stream, fishy operates at the collection level: given a baseline collection and a test collection — each comprising event streams from multiple named sources — it produces a structured anomaly report with an overall score, per-source attribution, and per-method evidence breakdown.

The core contribution is a two-level fusion architecture. At the first level, six independent analytical methods — distributional divergence, cross-source dependency shift, spectral fingerprinting, wavelet energy analysis, co-occurrence structure comparison, and evidence conflict — each measure a fundamentally different property of the collection pair. At the second level, Dempster-Shafer evidence theory combines the outputs into a fused belief. Each method's entropy measurement is treated as a first-class observable: it determines whether the method is in a productive operating regime (applicability gating) and, when multiple baselines are available, the entropy delta between baseline and test serves as an independent anomaly signal alongside the divergence score.

Calibration is intrinsic: with three or more baseline collections, BPAs are constructed from empirical percentiles of pairwise baseline divergences, requiring no training labels, no learned parameters, and no external thresholds. fishy is implemented in pure Rust with no deep learning dependencies and runs on any platform.

---

## 1. Introduction

Log-based anomaly detection is a mature field, but the dominant paradigm — classifying individual log events or sessions as normal or anomalous within a single ongoing stream — does not address a common operational need: comparing two complete, bounded log collections to determine whether they represent the same system behavior.

Consider the following scenarios:

- **Deployment validation**: A new software version is deployed to a canary instance. Do the logs from the canary look like the logs from the stable deployment?
- **Incident investigation**: An incident occurred between 14:00 and 16:00. Do the logs from that window look like the same window from the previous week?
- **Regression testing**: A test suite was run against two builds. Did the system behave the same way?
- **Compliance auditing**: Do the logs from the audited period show unexplained behavioral changes relative to a known-good baseline?

In all these cases, the analyst has two complete collections and wants a structured comparison — not a real-time stream classifier. Existing multi-source methods (LogMS [1], CoLog [2], HitAnomaly [3]) assume a single ongoing system being monitored and focus on event-level or session-level anomaly labels. They treat multi-source fusion as a way to improve single-event classification accuracy, not as the primary analytical lens.

fishy addresses this gap with three key design decisions:

1. **Collection comparison as the unit of analysis.** The input is two bounded collections; the output is a divergence score with attribution. No streaming, no training, no persistent state.

2. **Multi-source, multi-function evidence gathering.** Six analytical methods, each measuring a fundamentally different property (distribution shape, inter-source dependency, temporal periodicity, wavelet energy, graph structure, evidence agreement), are applied across all sources. The result is a matrix of observations — methods × sources — rather than a single feature vector.

3. **Entropy as observation, not weight.** Each method's entropy measurement is an independent observable. It determines whether the method is in a productive operating regime (applicability gating) and, when multiple baselines are available, the entropy delta between baseline and test is a first-class anomaly signal alongside the divergence score.

The remainder of this paper is organized as follows. Section 2 formalizes the collection comparison problem. Section 3 describes the six analysis methods. Section 4 presents the adaptive fusion pipeline. Section 5 covers multi-baseline variance estimation. Section 6 describes the implementation. Section 7 presents evaluation results on the AIT-LDSv2 dataset. Section 8 discusses limitations and future work. Section 9 concludes.

---

## 2. Problem Formulation

### 2.1 Collections and Sources

A **log collection** is a set of named event streams with collection-level timestamps:

$$C = \{(s_i, E_i)\}_{i=1}^{n}, \quad \text{metadata: } (t_{\text{start}}, t_{\text{end}})$$

where $s_i$ is a source identifier and $E_i = [(e_1, \tau_1), \ldots, (e_k, \tau_k)]$ is a sequence of events with template IDs $e_j \in \{1, \ldots, T\}$ and optional timestamps $\tau_j \in \mathbb{R}_{\geq 0}$.

Template IDs are assigned by the encoder (Section 6.2): each distinct log message pattern receives a unique integer ID, with $\text{TemplateId}(0)$ reserved for unknown patterns.

### 2.2 The Comparison Problem

Given a baseline collection $C_b$ and a test collection $C_t$, the goal is to compute:

$$\text{detect}(C_b, C_t) \to (s, u, \mathcal{A})$$

where $s \in [0,1]$ is the anomaly score, $u \in [0,1]$ is the uncertainty mass, and $\mathcal{A}$ is an attribution structure mapping sources and methods to their contributions.

With multiple baselines $\{C_b^{(1)}, \ldots, C_b^{(K)}\}$, the test is scored against the nearest baseline (Section 5).

### 2.3 Comparison Modes

**Single-Origin (SO)**: Both collections come from the same system. All sources present in the baseline must also be present in the test. A missing source is treated as maximum divergence.

**Multi-Origin (MO)**: Collections come from comparable but different systems. Only overlapping sources are compared; non-overlapping sources are ignored.

### 2.4 Temporal Alignment

Collections must cover roughly the same time duration. fishy validates that:

$$\frac{|d_b - d_t|}{d_b} \leq \delta$$

where $d_b = t_{\text{end}}^b - t_{\text{start}}^b$, $d_t = t_{\text{end}}^t - t_{\text{start}}^t$, and $\delta = 0.5$ by default. This constraint ensures that frequency-domain analysis has comparable spectral resolution in both collections.

---

## 3. Analysis Methods

fishy applies six independent analytical methods to each collection pair. Each method $M$ produces a divergence score $d_M \in [0,1]$ and an entropy value $H_M$ used for applicability gating and (with multiple baselines) an entropy delta $\Delta H_M = H_M(C_t) - H_M(C_b)$.

### 3.1 Applicability Gating

Each method has a natural entropy measure that characterizes its operating regime. Methods at entropy extremes are excluded:

- **Max entropy** (disorder): the method is blind. A uniform template distribution, white-noise spectrum, or random co-occurrence graph gives the method nothing to compare against.
- **Min entropy** (trivial order): the method sees structure but it is trivially simple — one dominant template, one frequency, one hub node.

The gate uses normalized entropy $\hat{H}_M \in [0,1]$ relative to the theoretical maximum for the method's representation:

$$\text{applicable}(M) = \hat{H}_M \in [\gamma_{\text{low}}, \gamma_{\text{high}}]$$

with $\gamma_{\text{low}} = 0.05$ and $\gamma_{\text{high}} = 0.95$.

### 3.2 Distributional Divergence (dist)

**Representation**: Template frequency distribution $P = \{p_e\}_{e=1}^{T}$ where $p_e = \text{count}(e) / \text{total}$.

**Divergence**: Jensen-Shannon divergence between baseline and test distributions:

$$d_{\text{dist}} = \text{JSD}(P_b \| P_t) = \frac{1}{2} D_{\text{KL}}(P_b \| M) + \frac{1}{2} D_{\text{KL}}(P_t \| M)$$

where $M = (P_b + P_t)/2$. JSD is symmetric and bounded in $[0, \log 2]$; fishy normalizes to $[0,1]$.

**Entropy measure**: Shannon entropy $H_{\text{dist}} = -\sum_e p_e \log p_e$.

**Applicability**: Always applicable (no minimum source count required).

**Attribution**: Per-template contribution to JSD, surfaced as `top_events` in `SourceReport`.

### 3.3 Cross-Source Dependency Shift (dep)

**Representation**: Mutual information matrix $M \in \mathbb{R}^{n \times n}$ where $M_{ij} = I(S_i; S_j)$ is the mutual information between sources $i$ and $j$, estimated from time-binned event counts.

**Divergence**: Normalized Frobenius distance:

$$d_{\text{dep}} = \frac{\|M_b - M_t\|_F}{\|M_b\|_F + \epsilon}$$

**Entropy measure**: Shannon entropy of the upper-triangular MI values, normalized by $\log\binom{n}{2}$.

**Applicability**: Requires $\geq 2$ sources.

**Motivation**: A database that stops correlating with the application server — even if both look individually normal — indicates a structural change. The MI matrix is the fingerprint of how system components interact.

### 3.4 Spectral Fingerprinting (spec)

**Representation**: Power spectrum $\Phi = \{|\hat{x}(f)|^2\}$ of the event rate time series $x[t]$ (events per time bin), computed via FFT.

**Divergence**: JSD between normalized power spectra:

$$d_{\text{spec}} = \text{JSD}(\Phi_b \| \Phi_t)$$

**Entropy measure**: Spectral flatness (Wiener entropy):

$$H_{\text{spec}} = \frac{\exp\left(\frac{1}{N}\sum_k \log \Phi[k]\right)}{\frac{1}{N}\sum_k \Phi[k]}$$

High spectral flatness indicates white noise (method blind); low flatness indicates a single dominant frequency (trivially simple).

**Applicability**: Requires $\geq 32$ events in at least one source.

**Motivation**: Periodic processes — cron jobs, health checks, batch processes — appear as peaks in the power spectrum. Changes in periodicity, amplitude, or the appearance/disappearance of periodic components are captured here.

### 3.5 Wavelet Energy Analysis (wavelet)

**Representation**: Discrete wavelet transform (Haar) of the event rate time series, decomposed into $L$ levels. Each level captures structure at a different temporal scale.

**Divergence**: Energy divergence across levels:

$$d_{\text{wavelet}} = \frac{1}{L} \sum_{\ell=1}^{L} \left| \frac{E_\ell^b}{\sum_k E_k^b} - \frac{E_\ell^t}{\sum_k E_k^t} \right|$$

where $E_\ell = \sum_j w_{\ell,j}^2$ is the energy at level $\ell$.

**Entropy measure**: Shannon entropy of the normalized energy distribution across levels.

**Applicability**: Requires $\geq 32$ events in at least one source.

**Motivation**: Wavelet decomposition provides multi-resolution analysis — long-term trends and short-term anomalies simultaneously. A pattern that was stable in the baseline but became erratic in the test appears as a change in the energy distribution across scales.

### 3.6 Co-occurrence Structure (co)

**Representation**: Event co-occurrence graph $G = (V, E)$ where nodes are template IDs and edge weights are co-occurrence frequencies within a sliding window. The normalized Laplacian $\mathcal{L} = D^{-1/2} L D^{-1/2}$ has eigenvalue spectrum $\lambda_1 \leq \ldots \leq \lambda_{|V|}$.

**Divergence**: JSD between normalized eigenvalue distributions:

$$d_{\text{co}} = \text{JSD}(\lambda_b \| \lambda_t)$$

**Entropy measure**: Von Neumann entropy of the normalized Laplacian:

$$H_{\text{co}} = -\text{tr}\left(\frac{\mathcal{L}}{\text{tr}(\mathcal{L})} \log \frac{\mathcal{L}}{\text{tr}(\mathcal{L})}\right)$$

**Applicability**: Requires $\geq 32$ events. Capped at `MAX_CO_NODES = 128` template types to bound eigendecomposition cost ($O(n^3)$).

**Motivation**: The eigenvalue spectrum of the Laplacian captures higher-order structural properties of the co-occurrence graph — connectivity, clustering, community structure — that individual event frequencies miss.

### 3.7 Evidence Conflict (conflict)

**Representation**: For each source $s_i$, a per-source BPA $m_i$ is constructed from the distributional divergence of that source alone, using `evidence_bpa(divergence, confidence)`.

**Divergence**: The conflict mass when combining all per-source BPAs via Dempster's rule:

$$d_{\text{conflict}} = k = 1 - \sum_{A \cap B \neq \emptyset} m_i(A) \cdot m_j(B)$$

**Entropy measure**: Shannon entropy of the per-source belief distribution $\{m_i(\{\text{anomalous}\})\}_{i=1}^n$.

**Applicability**: Requires $\geq 2$ sources.

**Motivation**: High conflict means the sources disagree about whether the system is normal. This is itself an anomaly signal — a system where some sources look normal and others look anomalous is behaving differently from a system where all sources agree.

---

## 4. Adaptive Fusion Pipeline

### 4.1 Overview

The adaptive fusion pipeline takes a set of baselines and a test collection and produces an `AnomalyReport`. The pipeline has seven stages:

1. Source pair matching (SO/MO mode resolution)
2. Representation extraction
3. Applicability gating
4. Baseline variance estimation
5. BPA construction
6. DS meta-fusion
7. Report assembly

### 4.2 Representation Extraction

All six methods share a single extraction pass over the collections. The `extract()` function converts a `LogCollection` into a `Representations` struct containing:

- Per-source template frequency distributions (for dist, conflict)
- Timed event sequences `(TemplateId, timestamp)` per source (for dep, spec, wavelet, co)
- Aggregated event times across all sources (for spectral/wavelet aggregate signals)

This single-pass design ensures that all methods see the same data and that extraction cost is paid once.

### 4.3 BPA Construction

Each applicable method $M$ produces two observations: a divergence score $d_M$ and (with multiple baselines) an entropy delta $\Delta H_M$. These are converted to BPAs over the frame of discernment $\Theta = \{\text{normal}, \text{anomalous}\}$, with mass on $\Theta$ representing uncertainty.

**Single-baseline path (sigmoid fallback)**

With one baseline, variance is estimated from a quarter-split of the baseline collection (three within-collection sample pairs). Only the divergence signal is used; $\Delta H_M$ is suppressed because within-collection entropy variance severely underestimates between-collection entropy variance, causing false positives.

The z-score is:

$$z_M = \frac{d_M - \mu_d}{\sigma_d + \epsilon}$$

and the BPA is constructed via sigmoid mapping:

$$m(\{\text{anomalous}\}) = \sigma(z_M - \theta_M), \quad m(\Theta) = 1 - m(\{\text{anomalous}\})$$

where $\theta_M$ is a per-method midpoint and $\sigma$ is the logistic function.

**Multi-baseline path (empirical CDF)**

With $K \geq 3$ baselines, variance is estimated from all $\binom{K}{2}$ pairwise baseline divergences. Both divergence and entropy delta are used.

The commitment for divergence is the empirical percentile:

$$p_M^d = \frac{|\{(i,j) : d_M(C_b^{(i)}, C_b^{(j)}) < d_M(C_b^*, C_t)\}|}{\binom{K}{2}}$$

where $C_b^*$ is the nearest baseline (Section 5.2). Similarly for $\Delta H_M$.

The BPA is:

$$m(\{\text{anomalous}\}) = p_M, \quad m(\Theta) = 1 - p_M$$

This mapping requires no tuned parameters: the empirical distribution of normal divergences is the calibration.

### 4.4 DS Meta-Fusion

All BPAs from applicable methods are combined via Dempster's rule of combination. For two BPAs $m_1$ and $m_2$:

$$(m_1 \oplus m_2)(A) = \frac{\sum_{B \cap C = A} m_1(B) \cdot m_2(C)}{1 - K}$$

where $K = \sum_{B \cap C = \emptyset} m_1(B) \cdot m_2(C)$ is the conflict mass.

`ds_combine_many` applies this iteratively over all BPAs. The final fused belief gives:

$$s = m(\{\text{anomalous}\}), \quad u = m(\Theta), \quad k_{\text{meta}} = K$$

The meta-conflict $k_{\text{meta}}$ is reported as a signal: high conflict between methods with committed BPAs (low baseline variance) indicates that the analytical lenses see genuinely different things, which is informative for investigation.

### 4.5 Score Interpretation

The anomaly score $s \in [0,1]$ maps to a verdict string:

| Score range | Verdict |
|---|---|
| $[0.00, 0.20)$ | looks clean |
| $[0.20, 0.40)$ | probably fine |
| $[0.40, 0.60)$ | worth a look |
| $[0.60, 0.80)$ | something smells off |
| $[0.80, 1.00]$ | definitely fishy |

The threshold for the binary exit code (0 = clean, 1 = anomalous) is configurable (default 0.5).

---

## 5. Multi-Baseline Variance Estimation

### 5.1 Motivation

With a single baseline, variance is estimated from within-collection quarter-splits. This captures within-collection noise but misses between-collection drift — day-to-day variation in normal system behavior. When the noise floor is too tight, gradual drift between normal days appears anomalous.

With multiple baseline collections, pairwise divergences between baselines provide a direct estimate of between-collection variance, giving a realistic noise floor.

### 5.2 Nearest-Baseline Scoring

With $K$ baselines, the test is scored against its nearest baseline rather than a fixed reference:

$$C_b^* = \arg\min_{C_b^{(k)}} d_M(C_b^{(k)}, C_t)$$

This prevents startup-day artifacts or atypical baseline days from inflating scores when better baselines exist.

### 5.3 Outlier Baseline Rejection

A baseline $C_b^{(k)}$ is flagged as an outlier if its mean divergence to the other baselines exceeds two standard deviations from the group mean:

$$\bar{d}_k = \frac{1}{K-1} \sum_{j \neq k} d_M(C_b^{(k)}, C_b^{(j)})$$

$$\text{outlier}(k) = \bar{d}_k > \mu_{\bar{d}} + 2\sigma_{\bar{d}}$$

Outlier baselines are flagged in the report (`rejected_baselines`) and excluded from variance estimation. This handles cases like startup days with unusual initialization artifacts.

### 5.4 Trend Detection

With $K \geq 3$ ordered baselines, a linear trend can be fitted to the per-method divergences across baselines:

$$\hat{d}_M(k) = \alpha_M + \beta_M \cdot k$$

The trend z-score for the test is:

$$z_M^{\text{trend}} = \frac{d_M(C_b^*, C_t) - \hat{d}_M(K+1)}{\text{SE}_M}$$

where $\text{SE}_M$ is the standard error of the prediction. A test that continues a monotonic trend ($|z_M^{\text{trend}}|$ small) is less anomalous than one that breaks from it. The trend z-score is reported in `MethodDetail` as `trend_z` and can optionally modulate the divergence BPA.

---

## 6. Implementation

### 6.1 Workspace Structure

fishy is a Cargo workspace with three crates:

- **`analysis/`**: Stateless mathematical functions. No domain types, no I/O. Operates on `&[f64]`, `EventDistribution`, `MIMatrix`, etc. — never on `LogCollection`. This crate boundary is enforced: `analysis` never imports `fishy` types.

- **`fishy/`**: Orchestration layer. Owns domain types (`LogCollection`, `DetectConfig`, `AnomalyReport`) and the full adaptive fusion pipeline. Depends on `analysis`.

- **`encoder/`**: Log tokenization. Converts raw log files into fishy's JSON collection format. Depends on `fishy` for the output format.

### 6.2 Encoder: Drain Template Extraction

The encoder uses a Drain parse tree [4] for template extraction. Drain groups log messages by token count and first-token prefix, then merges groups by token similarity (threshold 0.5). Variable tokens are replaced with wildcards.

The encoder operates in two passes:

1. **`build-dict`**: Trains the Drain tree on the baseline logs, assigns frequency-ranked `TemplateId` values (most frequent template → `TemplateId(1)`), and writes `dict.json` + `drain.json`.

2. **`encode`**: Loads the trained tree and dictionary, classifies each log line to a template ID, extracts timestamps (auto-detecting ISO 8601, syslog, nginx/apache, unix seconds, and JSON timestamp fields), and writes the collection directory.

The shared dictionary guarantees cross-collection consistency: the same log line produces the same `TemplateId` in both baseline and test, preventing false divergence from template drift.

### 6.3 Collection Format

Each collection is a directory:

```
collection/
├── meta.json          {"start_time": u64, "end_time": u64}
├── 0.json             {"events": [{"template_id": u32, "timestamp": u64, "params": {}}]}
├── 1.json
└── ...
```

Source files are named by integer index corresponding to sorted source names. Timestamps are relative seconds from collection start.

### 6.4 Synthetic Test Scenarios

The `gen` binary generates synthetic collections for calibration and regression testing. Nine severity-graded scenarios are provided:

- **clean**: Baseline-like collections with natural within-collection variance
- **dist_shift**: Template frequency distribution changes
- **dep_break**: Cross-source dependency structure changes
- **spectral_shift**: Temporal periodicity changes
- **conflict**: Sources disagree (some normal, some anomalous)
- **multi_anomaly**: Multiple simultaneous anomaly types
- Additional severity variants at 25%, 50%, and 75% intensity

These scenarios validate that each method fires on its intended signal and that the fusion correctly combines evidence from multiple simultaneous anomalies.

### 6.5 AIT-LDSv2 Preprocessing

The `prep_ait.py` script converts AIT-LDSv2 [5] scenarios directly to fishy's JSON format, bypassing the encoder. It handles six log types (syslog, Apache access logs, Suricata JSON alerts, Linux audit logs, OpenVPN logs, dnsmasq logs) and splits each scenario into day-level collections. Attack times are hardcoded for all eight scenarios.

---

## 7. Evaluation

### 7.1 Dataset

We evaluate on the AIT-LDSv2 dataset [5], a realistic multi-host enterprise network simulation with eight attack scenarios. Each scenario spans 4–6 days with 3–4 normal days followed by an attack day. The dataset provides ground truth at the event level, which we aggregate to the collection level: a collection is labeled anomalous if it contains any attack-phase events.

We use the **russellmitchell** and **santos** scenarios for primary evaluation, providing 3 normal baseline days and 1 attack day each.

### 7.2 Experimental Setup

**Single-baseline evaluation**: For each scenario, we use day 1 as the baseline and evaluate days 2 (normal) and 3 (attack) as test collections. This tests the basic detection capability.

**Multi-baseline evaluation**: We use days 1–3 as baselines and day 4 (attack) as the test. This tests whether multi-baseline calibration reduces false positives from day-to-day drift.

**Metrics**:
- **Detection accuracy**: AUC-ROC over collection pairs (normal-vs-normal vs normal-vs-attack)
- **Score separation**: Cohen's $d$ between score distributions for normal and attack pairs
- **False positive rate**: Fraction of normal-vs-normal pairs scoring above threshold 0.5

### 7.3 Results

| Scenario | Mode | AUC-ROC | Cohen's $d$ | FPR@0.5 |
|---|---|---|---|---|
| russellmitchell | Single baseline | — | — | — |
| russellmitchell | Multi-baseline (3) | — | — | — |
| santos | Single baseline | — | — | — |
| santos | Multi-baseline (3) | — | — | — |

*Results pending full evaluation run. Qualitative findings: multi-baseline calibration substantially reduces false positives from day-to-day drift (the day_0 startup artifact that inflated single-baseline scores is absorbed into the between-collection variance estimate). The dep and conflict methods show the strongest signal on the russellmitchell scenario, consistent with the attack involving lateral movement that disrupts cross-source dependency structure.*

### 7.4 Per-Method Contribution

The ablation design (Section 8.2) will quantify the contribution of each method. Qualitatively:

- **dist** fires on all scenarios (template frequency changes are a universal signal)
- **dep** and **conflict** are most sensitive to attacks involving multiple hosts (lateral movement, C2 communication)
- **spec** and **wavelet** detect timing-based anomalies (beaconing, scheduled exfiltration)
- **co** detects structural changes in event co-occurrence patterns (new attack-phase event sequences)

---

## 8. Discussion

### 8.1 Limitations

**Template vocabulary drift.** If the test collection contains log messages not seen during dictionary construction, they receive `TemplateId(0)`. In SO mode this is correctly treated as anomalous (new event types appeared). In MO mode it is ambiguous — new code paths may be legitimate. The current implementation treats unknown templates as a separate category in the distributional divergence computation.

**Parameter-only anomalies.** The encoder discards parameter values after template extraction. An anomaly that manifests only in parameter values (e.g., an unusual IP address in an otherwise normal log pattern) is invisible to all six methods. Mitigation requires parameter distribution extraction, which is deferred to future work.

**Uniform shift.** If all sources shift identically (e.g., a system-wide configuration change), the evidence conflict and dependency shift methods produce near-zero divergence. Only distributional and spectral methods fire. The multi-source advantage disappears in this case.

**Non-stationary baselines.** A baseline with a regime change (e.g., a deployment mid-baseline) inflates sub-sample variance, suppressing all methods' BPA commitment. The outlier baseline rejection partially addresses this for multi-baseline scenarios, but within-collection non-stationarity is not detected.

**Small collections.** The applicability gate (minimum 32 events for spectral, wavelet, and co methods) prevents degenerate analysis, but very small collections may have only the distributional method active, reducing fusion quality.

### 8.2 Ablation Study Design

The following ablations are planned to quantify each component's contribution:

1. **Single-method baselines**: Run each method alone (no fusion). Measures the value of fusion over individual methods.
2. **Equal-weight fusion**: Replace empirical CDF with uniform BPAs. Measures the value of signal-to-noise calibration.
3. **Remove entropy-delta BPAs**: Use divergence BPAs only. Measures the value of entropy-as-observation.
4. **Remove applicability gating**: Run all methods regardless of entropy regime. Measures the value of gating.
5. **Remove outlier rejection**: Include all baselines in variance estimation. Measures the value of outlier detection.

### 8.3 Relationship to Existing Frameworks

**JDL/DFIG model.** fishy spans JDL Levels 1–4 in a single function call. The six analysis methods correspond to L1 (object assessment) and L2 (situation assessment) running in parallel. DS meta-fusion is L3 (impact assessment). Applicability gating and baseline calibration are L4 (process refinement) executed as pre-fusion steps rather than post-fusion feedback loops.

**Dasarathy model.** fishy is a two-stage pipeline: multiple parallel DAI-FEO paths (analysis methods extracting features from event data) feeding into FEI-FEO calibration (BPA construction from signal-to-noise) and DEI-DEO combination (DS fusion).

**Multi-domain entropy approaches.** Existing work (e.g., [6]) computes multiple entropy types from the same signal and concatenates them as features for supervised classifiers. fishy's approach is structurally different: entropy is measured per analytical lens as an independent observable in DS-theoretic fusion, and entropy delta is a first-class anomaly signal rather than a feature.

### 8.4 Future Work

**Drain as default encoder.** The current encoder uses Drain for template extraction. Future work should validate Drain's accuracy on the specific log formats in AIT-LDSv2 and compare against the regex-based fallback.

**Pluggable lens architecture.** The analytical methods are currently hardcoded. A trait-based pluggable architecture (`AnalyticalLens<C>`) would allow domain-specific lenses to be added without modifying the fusion engine, enabling fishy's framework to be applied to non-log data (e.g., tabular time series from baseband traces).

**Empirical applicability thresholds.** The gate thresholds $\gamma_{\text{low}} = 0.05$ and $\gamma_{\text{high}} = 0.95$ are conservative defaults. Empirical validation on AIT-LDSv2 should identify per-method thresholds that maximize detection accuracy.

**Statistical hypothesis testing.** The fused belief score could be calibrated to a p-value via permutation testing: shuffle events between baseline and test, recompute the score, build an empirical null distribution. This would give the output statistical meaning ("the probability of seeing this much divergence by chance is $p$").

---

## 9. Conclusion

We presented fishy, a system for multi-source log collection anomaly detection through information-theoretic evidence fusion. The key contributions are:

1. **Collection comparison as the unit of analysis.** fishy compares two bounded multi-source collections rather than classifying individual events or sessions. This addresses a common operational need — deployment validation, incident investigation, regression testing — that existing stream-based methods do not.

2. **Multi-source, multi-function evidence gathering.** Six analytical methods, each measuring a fundamentally different property, are applied across all sources. The result is a matrix of observations that captures distributional, structural, temporal, and relational changes simultaneously.

3. **Entropy as observation.** Each method's entropy measurement is a first-class observable used for applicability gating and (with multiple baselines) as an independent anomaly signal via entropy delta. This is structurally different from existing multi-domain entropy approaches that treat entropy as a feature for supervised classifiers.

4. **Self-calibrating from baseline variance.** With three or more baseline collections, BPAs are constructed from empirical percentiles of pairwise baseline divergences. No training labels, no learned parameters, no external thresholds. The calibration is intrinsic to the data.

5. **Pure Rust implementation.** fishy runs on any platform without deep learning dependencies, GPU requirements, or training infrastructure.

---

## References

[1] LogMS: Multi-Source Information Fusion-based LSTM for Log Anomaly Detection. *Frontiers in Physics*, 2024.

[2] CoLog: Collaborative Transformers for Multi-Source Log Anomaly Detection. *Nature Scientific Reports*, 2025.

[3] HitAnomaly: Hierarchical Transformers for Anomaly Detection in System Log. *IEEE Transactions*, 2020.

[4] He, P., Zhu, J., Zheng, Z., Lyu, M.R. Drain: An Online Log Parsing Approach with Fixed Depth Tree. *IEEE ICWS*, 2017.

[5] Landauer, M., et al. A Comprehensive Survey on Log-based Anomaly Detection. *ACM Computing Surveys*, 2023. AIT-LDSv2 dataset: https://zenodo.org/record/5789064

[6] Multi-Domain Entropy-Random Forest Method for Bearing Fault Diagnosis. *MDPI Entropy*, 22(1):57, 2020.

[7] Dempster, A.P. A Generalization of Bayesian Inference. *Journal of the Royal Statistical Society*, 1968.

[8] Shafer, G. *A Mathematical Theory of Evidence*. Princeton University Press, 1976.

[9] Gretton, A., et al. A Kernel Two-Sample Test. *JMLR*, 13:723–773, 2012.

[10] Muñoz, A., et al. Combining Entropy Measures for Anomaly Detection. *Entropy*, 20(9):698, 2018.
