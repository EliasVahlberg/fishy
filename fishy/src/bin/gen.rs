//! Synthetic log collection generator for fishy development and testing.
//!
//! Usage: `cargo run --bin gen`
//!
//! Writes `testdata/<scenario>/baseline/` and `testdata/<scenario>/test/`.
//!
//! # Scenarios
//!
//! | Scenario        | Expected score | Primary signal                          |
//! |-----------------|----------------|-----------------------------------------|
//! | clean           | ≈ 0.00         | none — identical collections            |
//! | dist_shift      | ≈ 0.18         | dist: template distribution flips       |
//! | dep_break       | ≈ 0.10         | dep: synchronized sources desynchronize |
//! | spectral_shift  | ≈ 0.68         | spec/wavelet: periodic → random timing  |
//! | conflict        | ≈ 0.24         | conflict: sources disagree on normality |
//! | multi_anomaly   | ≈ 0.60         | dist + spec + missing source            |

use serde_json::{json, Value};
use std::path::Path;

fn main() {
    write_scenario("clean",          baseline(),           baseline());
    write_scenario("dist_shift",     baseline(),           dist_shift());
    write_scenario("dep_break",      dep_break_baseline(), dep_break_test());
    write_scenario("spectral_shift", spectral_baseline(),  spectral_test());
    write_scenario("conflict",       conflict_baseline(),  conflict_test());
    write_scenario("multi_anomaly",  baseline(),           multi_anomaly());
    println!("testdata/ written — run each with:");
    println!("  cargo run --bin fishy -- -b testdata/<scenario>/baseline -c testdata/<scenario>/test -v");
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

/// Stable baseline: 3 sources, 1-hour window, periodic health-checks + background noise.
fn baseline() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(1, 40), (2, 30), (3, 20), (4, 10)], 120, 42)),
        (1, periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7)),
        (2, periodic_source(180, &[(1, 35), (3, 35), (8, 30)], 180, 13)),
    ]
}

/// Source 0 distribution flips; sources 1 and 2 unchanged.
/// Expected: dist fires, dep/spec/conflict quiet.
fn dist_shift() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(9, 70), (2, 20), (4, 10)], 120, 42)),
        (1, periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7)),
        (2, periodic_source(180, &[(1, 35), (3, 35), (8, 30)], 180, 13)),
    ]
}

/// Baseline: sources 0 and 1 fire at the same times with the same template (correlated).
/// Source 2 fires independently. 3 sources needed: matrix_entropy of a 2-source MI
/// matrix is always 0 (upper triangle has one value → entropy of [1.0] = 0).
/// Test: all sources fire independently (different templates at same times).
/// Same marginal template distribution in both → dist ≈ 0, dep fires.
fn dep_break_baseline() -> Vec<(u32, Value)> {
    let mut rng = Lcg::new(55);
    let mut rng2 = Lcg::new(99);
    let (mut e0, mut e1, mut e2) = (vec![], vec![], vec![]);
    for t in (0u64..3600).step_by(10) {
        let tid = (rng.next_u64() >> 62) as u32 + 1;
        e0.push(json!({"template_id": tid, "timestamp": t, "params": {}}));
        e1.push(json!({"template_id": tid, "timestamp": t, "params": {}})); // same template
        // Source 2: same template as source 0 half the time (partial correlation → MI(0,2) > 0)
        let r = rng2.next_u64();
        let tid2 = if r >> 63 == 0 { tid } else { (r >> 62) as u32 + 1 };
        e2.push(json!({"template_id": tid2, "timestamp": t, "params": {}}));
    }
    vec![
        (0, json!({"events": e0})),
        (1, json!({"events": e1})),
        (2, json!({"events": e2})),
    ]
}

fn dep_break_test() -> Vec<(u32, Value)> {
    // All sources fire independently — different templates at the same times.
    let mut rng0 = Lcg::new(55);
    let mut rng1 = Lcg::new(66); // different seed → uncorrelated templates
    let mut rng2 = Lcg::new(99);
    let (mut e0, mut e1, mut e2) = (vec![], vec![], vec![]);
    for t in (0u64..3600).step_by(10) {
        let tid0 = (rng0.next_u64() >> 62) as u32 + 1;
        let tid1 = (rng1.next_u64() >> 62) as u32 + 1;
        let tid2 = (rng2.next_u64() >> 62) as u32 + 1;
        e0.push(json!({"template_id": tid0, "timestamp": t, "params": {}}));
        e1.push(json!({"template_id": tid1, "timestamp": t, "params": {}}));
        e2.push(json!({"template_id": tid2, "timestamp": t, "params": {}}));
    }
    vec![
        (0, json!({"events": e0})),
        (1, json!({"events": e1})),
        (2, json!({"events": e2})),
    ]
}

/// Baseline: events at regular 45s intervals (strong periodic signal).
/// Test: same count, random timing (no periodicity).
/// Same template distribution → dist ≈ 0, spec/wavelet fire.
fn spectral_baseline() -> Vec<(u32, Value)> {
    let times: Vec<u64> = (0u64..3600).step_by(45).collect();
    vec![(0, timed_source(&times, 1))]
}

fn spectral_test() -> Vec<(u32, Value)> {
    let mut rng = Lcg::new(77);
    let n = (3600u64 / 45) as usize; // same event count
    let times: Vec<u64> = (0..n).map(|_| rng.next_u64() % 3600).collect();
    vec![(0, timed_source(&times, 1))]
}

/// Baseline: 3 sources with identical distributions.
/// Test: source 1 has a completely different distribution; sources 0 and 2 unchanged.
/// DS conflict between source 1's BPA and sources 0/2 should be high.
fn conflict_baseline() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(1, 50), (2, 50)], 120, 11)),
        (1, periodic_source(200, &[(1, 50), (2, 50)], 120, 22)),
        (2, periodic_source(200, &[(1, 50), (2, 50)], 120, 33)),
    ]
}

fn conflict_test() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(1, 50), (2, 50)], 120, 11)),
        (1, periodic_source(200, &[(9, 95), (10, 5)], 120, 22)), // completely different
        (2, periodic_source(200, &[(1, 50), (2, 50)], 120, 33)),
    ]
}

/// Source 0 shifts + source 1 gains a new fast periodic process + source 2 missing.
fn multi_anomaly() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(9, 70), (2, 20), (4, 10)], 120, 42)),
        (1, {
            let mut s = periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7);
            let extra: Vec<Value> = (0u64..3600).step_by(60).flat_map(|t| {
                (0..3u64).map(move |off| {
                    json!({"template_id": 10, "timestamp": t + off, "params": {}})
                })
            }).collect();
            s["events"].as_array_mut().unwrap().extend(extra);
            s
        }),
        // Source 2 absent — maximum divergence in SO mode.
    ]
}

// ---------------------------------------------------------------------------
// Event builders
// ---------------------------------------------------------------------------

/// Source with background noise + a periodic heartbeat at `period` seconds.
fn periodic_source(count: u32, weights: &[(u32, u32)], period: u64, seed: u64) -> Value {
    let mut rng = Lcg::new(seed);
    let total_weight: u32 = weights.iter().map(|(_, w)| w).sum();
    let mut events: Vec<Value> = (0..count)
        .map(|_| {
            let ts = rng.next_u64() % 3600;
            let tid = pick_template(&mut rng, weights, total_weight);
            json!({"template_id": tid, "timestamp": ts, "params": {}})
        })
        .collect();
    for t in (0..3600u64).step_by(period as usize) {
        events.push(json!({"template_id": 0, "timestamp": t, "params": {}}));
    }
    json!({"events": events})
}

/// Source with events at exactly the given timestamps, all using `template_id`.
fn timed_source(times: &[u64], template_id: u32) -> Value {
    let events: Vec<Value> = times
        .iter()
        .map(|&t| json!({"template_id": template_id, "timestamp": t, "params": {}}))
        .collect();
    json!({"events": events})
}

fn pick_template(rng: &mut Lcg, weights: &[(u32, u32)], total: u32) -> u32 {
    let r = (rng.next_u64() % total as u64) as u32;
    let mut acc = 0u32;
    for &(tid, w) in weights {
        acc += w;
        if r < acc { return tid; }
    }
    weights.last().unwrap().0
}

// ---------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------

fn write_scenario(name: &str, baseline: Vec<(u32, Value)>, test: Vec<(u32, Value)>) {
    write_collection(&format!("testdata/{name}/baseline"), baseline);
    write_collection(&format!("testdata/{name}/test"), test);
}

fn write_collection(dir: &str, sources: Vec<(u32, Value)>) {
    std::fs::create_dir_all(dir).unwrap();
    write_json(&format!("{dir}/meta.json"), &json!({"start_time": 0, "end_time": 3600}));
    for (id, stream) in sources {
        write_json(&format!("{dir}/{id}.json"), &stream);
    }
}

fn write_json(path: &str, value: &Value) {
    std::fs::write(Path::new(path), serde_json::to_string_pretty(value).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// Minimal LCG
// ---------------------------------------------------------------------------

struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self { Self(seed) }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
}
