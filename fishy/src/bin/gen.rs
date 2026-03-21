//! Synthetic log collection generator for fishy development and testing.
//!
//! Usage: `cargo run --bin gen`
//!
//! Writes `testdata/<scenario>/baseline/` and `testdata/<scenario>/test/`.
//!
//! # Scenarios
//!
//! - `clean`         — identical collections; expect score ≈ 0
//! - `dist_shift`    — source 0 template distribution shifts; expect score > 0.5
//! - `multi_anomaly` — distributional shift + new periodic process + missing source

use serde_json::{json, Value};
use std::path::Path;

fn main() {
    write_scenario("clean", baseline(), baseline());
    write_scenario("dist_shift", baseline(), dist_shift());
    write_scenario("multi_anomaly", baseline(), multi_anomaly());
    println!("testdata/ written.");
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
fn dist_shift() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(9, 70), (2, 20), (4, 10)], 120, 42)),
        (1, periodic_source(150, &[(5, 50), (6, 30), (7, 20)], 300, 7)),
        (2, periodic_source(180, &[(1, 35), (3, 35), (8, 30)], 180, 13)),
    ]
}

/// Source 0 shifts + source 1 gains a new fast periodic process + source 2 missing.
fn multi_anomaly() -> Vec<(u32, Value)> {
    vec![
        (0, periodic_source(200, &[(9, 70), (2, 20), (4, 10)], 120, 42)),
        // Source 1: same distribution but now also has a 60s periodic burst (template 10).
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

    // Background events: uniformly distributed timestamps.
    let mut events: Vec<Value> = (0..count)
        .map(|_| {
            let ts = rng.next_u64() % 3600;
            let tid = pick_template(&mut rng, weights, total_weight);
            json!({"template_id": tid, "timestamp": ts, "params": {}})
        })
        .collect();

    // Periodic heartbeat: template 0 fires every `period` seconds.
    for t in (0..3600u64).step_by(period as usize) {
        events.push(json!({"template_id": 0, "timestamp": t, "params": {}}));
    }

    json!({"events": events})
}

fn pick_template(rng: &mut Lcg, weights: &[(u32, u32)], total: u32) -> u32 {
    let r = (rng.next_u64() % total as u64) as u32;
    let mut acc = 0u32;
    for &(tid, w) in weights {
        acc += w;
        if r < acc {
            return tid;
        }
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
