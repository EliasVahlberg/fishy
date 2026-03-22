//! Synthetic log collection generator for fishy development and testing.
//!
//! Usage: `cargo run --bin gen`
//!
//! Writes `testdata/<scenario>/baseline/` and `testdata/<scenario>/test/`.
//!
//! # Scenarios
//!
//! | Scenario              | Primary signal                          |
//! |-----------------------|-----------------------------------------|
//! | clean                 | none — identical collections            |
//! | dist_mild             | dist: 1 of 5 sources shifts slightly    |
//! | dist_moderate         | dist: 2 of 5 sources shift              |
//! | dist_severe           | dist: 3 of 5 sources shift heavily      |
//! | spectral_mild         | wavelet: slight timing jitter           |
//! | spectral_severe       | spec/wavelet: periodic → random timing  |
//! | dep_break             | dep: synchronized sources desynchronize |
//! | conflict              | conflict: sources disagree on normality |
//! | multi_anomaly         | dist + spec + missing source            |

use serde_json::{json, Value};
use std::path::Path;

fn main() {
    let scenarios: Vec<(&str, Vec<(u32, Value)>, Vec<(u32, Value)>)> = vec![
        ("clean",            baseline_5(),        baseline_5()),
        ("dist_mild",        baseline_5(),        dist_mild()),
        ("dist_moderate",    baseline_5(),        dist_moderate()),
        ("dist_severe",      baseline_5(),        dist_severe()),
        ("spectral_mild",    spectral_baseline(), spectral_mild()),
        ("spectral_severe",  spectral_baseline(), spectral_severe()),
        ("dep_break",        dep_break_baseline(), dep_break_test()),
        ("conflict",         conflict_baseline(), conflict_test()),
        ("multi_anomaly",    baseline_5(),        multi_anomaly()),
    ];

    for (name, baseline, test) in scenarios {
        write_scenario(name, baseline, test);
    }

    println!("testdata/ written — run each with:");
    println!("  cargo run --bin fishy -- -b testdata/<scenario>/baseline -c testdata/<scenario>/test -v");
}

// ---------------------------------------------------------------------------
// 5-source baseline (more realistic than 3)
// ---------------------------------------------------------------------------

fn baseline_5() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(1, 40), (2, 30), (3, 20), (4, 10)], 120, 42)),
        (1, periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7)),
        (2, periodic_source(180, &[(1, 35), (3, 35), (8, 30)], 180, 13)),
        (3, periodic_source(160, &[(2, 45), (5, 35), (9, 20)], 240, 91)),
        (4, periodic_source(170, &[(6, 40), (7, 30), (3, 30)], 150, 53)),
    ]
}

// ---------------------------------------------------------------------------
// Distributional shift — graded severity
// ---------------------------------------------------------------------------

/// Mild: 1 of 5 sources has a small template shift (swap 10% weight).
fn dist_mild() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(1, 30), (2, 30), (3, 20), (4, 10), (9, 10)], 120, 42)),
        (1, periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7)),
        (2, periodic_source(180, &[(1, 35), (3, 35), (8, 30)], 180, 13)),
        (3, periodic_source(160, &[(2, 45), (5, 35), (9, 20)], 240, 91)),
        (4, periodic_source(170, &[(6, 40), (7, 30), (3, 30)], 150, 53)),
    ]
}

/// Moderate: 2 of 5 sources shift templates.
fn dist_moderate() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(9, 50), (2, 30), (4, 20)], 120, 42)),
        (1, periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7)),
        (2, periodic_source(180, &[(10, 50), (3, 30), (8, 20)], 180, 13)),
        (3, periodic_source(160, &[(2, 45), (5, 35), (9, 20)], 240, 91)),
        (4, periodic_source(170, &[(6, 40), (7, 30), (3, 30)], 150, 53)),
    ]
}

/// Severe: 3 of 5 sources shift heavily + new templates.
fn dist_severe() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(9, 70), (10, 20), (4, 10)], 120, 42)),
        (1, periodic_source(150, &[(11, 60), (12, 30), (7, 10)], 300, 7)),
        (2, periodic_source(180, &[(10, 50), (13, 30), (14, 20)], 180, 13)),
        (3, periodic_source(160, &[(2, 45), (5, 35), (9, 20)], 240, 91)),
        (4, periodic_source(170, &[(6, 40), (7, 30), (3, 30)], 150, 53)),
    ]
}

// ---------------------------------------------------------------------------
// Spectral shift — graded severity
// ---------------------------------------------------------------------------

fn spectral_baseline() -> Vec<(u32, Value)> {
    // 3 sources with strong periodic signals at different intervals
    vec![
        (0, timed_periodic(45, 3600, 1)),
        (1, timed_periodic(60, 3600, 2)),
        (2, timed_periodic(90, 3600, 3)),
    ]
}

/// Mild: add 20% timing jitter to one source.
fn spectral_mild() -> Vec<(u32, Value)> {
    vec![
        (0, timed_jittered(45, 3600, 1, 0.2, 77)),
        (1, timed_periodic(60, 3600, 2)),
        (2, timed_periodic(90, 3600, 3)),
    ]
}

/// Severe: one source goes fully random timing.
fn spectral_severe() -> Vec<(u32, Value)> {
    let mut rng = Lcg::new(77);
    let n = (3600u64 / 45) as usize;
    let times: Vec<u64> = (0..n).map(|_| rng.next_u64() % 3600).collect();
    vec![
        (0, timed_source(&times, 1)),
        (1, timed_periodic(60, 3600, 2)),
        (2, timed_periodic(90, 3600, 3)),
    ]
}

// ---------------------------------------------------------------------------
// Dependency break
// ---------------------------------------------------------------------------

fn dep_break_baseline() -> Vec<(u32, Value)> {
    let mut rng = Lcg::new(55);
    let mut rng2 = Lcg::new(99);
    let (mut e0, mut e1, mut e2) = (vec![], vec![], vec![]);
    for t in (0u64..3600).step_by(10) {
        let tid = (rng.next_u64() >> 62) as u32 + 1;
        e0.push(json!({"template_id": tid, "timestamp": t, "params": {}}));
        e1.push(json!({"template_id": tid, "timestamp": t, "params": {}}));
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
    let mut rng0 = Lcg::new(55);
    let mut rng1 = Lcg::new(66);
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

// ---------------------------------------------------------------------------
// Conflict
// ---------------------------------------------------------------------------

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
        (1, periodic_source(200, &[(9, 95), (10, 5)], 120, 22)),
        (2, periodic_source(200, &[(1, 50), (2, 50)], 120, 33)),
    ]
}

// ---------------------------------------------------------------------------
// Multi-anomaly
// ---------------------------------------------------------------------------

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
        // Sources 2-4 absent → missing source signal
    ]
}

// ---------------------------------------------------------------------------
// Event builders
// ---------------------------------------------------------------------------

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

fn timed_periodic(interval: u64, duration: u64, template_id: u32) -> Value {
    let times: Vec<u64> = (0..duration).step_by(interval as usize).collect();
    timed_source(&times, template_id)
}

fn timed_jittered(interval: u64, duration: u64, template_id: u32, jitter_frac: f64, seed: u64) -> Value {
    let mut rng = Lcg::new(seed);
    let max_jitter = (interval as f64 * jitter_frac) as u64;
    let times: Vec<u64> = (0..duration).step_by(interval as usize).map(|t| {
        let offset = if max_jitter > 0 { rng.next_u64() % (2 * max_jitter + 1) } else { 0 };
        (t as u64).saturating_add(offset).saturating_sub(max_jitter).min(duration - 1)
    }).collect();
    timed_source(&times, template_id)
}

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
