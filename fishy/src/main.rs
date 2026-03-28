use clap::Parser;
use fishy::{load_collection, AnomalyReport, DetectConfig};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fishy", about = "Multi-source anomaly detection")]
struct Cli {
    /// Baseline collection directory (repeat for multiple baselines)
    #[arg(short, long, num_args = 1..)]
    baseline: Vec<PathBuf>,

    /// Test collection directory to compare against baseline
    #[arg(short = 'c', long)]
    compare: PathBuf,

    /// Comparison mode: so (default) | mo
    #[arg(short, long, default_value = "so")]
    mode: String,

    /// Anomaly threshold, 0.0–1.0
    #[arg(short, long, default_value = "0.5")]
    threshold: f32,

    /// Show per-source breakdown
    #[arg(short, long)]
    verbose: bool,

    /// Output full AnomalyReport as JSON
    #[arg(long)]
    json: bool,

    /// Duration tolerance for temporal validation, 0.0 = disabled [default: 0.5]
    #[arg(long, default_value = "0.5")]
    duration_tolerance: f32,
}

fn main() {
    let cli = Cli::parse();

    if cli.baseline.is_empty() {
        eprintln!("error: at least one -b baseline directory required");
        std::process::exit(2);
    }

    let baselines: Vec<_> = cli.baseline.iter().map(|p| {
        load_collection(p).unwrap_or_else(|e| {
            eprintln!("error loading baseline {}: {e}", p.display());
            std::process::exit(1);
        })
    }).collect();

    let test = load_collection(&cli.compare).unwrap_or_else(|e| {
        eprintln!("error loading test collection: {e}");
        std::process::exit(1);
    });

    let mode = match cli.mode.as_str() {
        "mo" => fishy::ComparisonMode::MultiOrigin,
        _ => fishy::ComparisonMode::SingleOrigin,
    };

    let config = DetectConfig {
        mode,
        significance_threshold: cli.threshold,
        duration_tolerance: cli.duration_tolerance,
        ..DetectConfig::default()
    };

    match fishy::detect(&baselines, &test, &config) {
        Ok(report) => {
            let anomalous = report.score >= cli.threshold as f64;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("{}", report.verdict);
                if cli.verbose {
                    print_verbose(&report);
                }
            }
            if anomalous {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}

fn print_verbose(report: &AnomalyReport) {
    println!("  uncertainty: {:.2}", report.uncertainty);
    println!("  baselines: {} used", report.baseline_count);
    if !report.rejected_baselines.is_empty() {
        let ids: Vec<_> = report.rejected_baselines.iter().map(|i| i.to_string()).collect();
        println!("  rejected baselines (outliers): {}", ids.join(", "));
    }
    if report.meta_conflict > 0.05 {
        println!("  meta-conflict: {:.2} (methods disagree)", report.meta_conflict);
    }

    if !report.methods.is_empty() {
        println!("  methods:");
        for m in &report.methods {
            if m.applicable {
                let pct_str = match m.divergence_percentile {
                    Some(p) => format!("  pct={:.2}", p),
                    None => String::new(),
                };
                let trend_str = match m.trend_z {
                    Some(z) if z.abs() > 2.0 => format!("  trend_z={:+.1}", z),
                    _ => String::new(),
                };
                println!(
                    "    {:>10}: div={:.2}  ΔH={:+.3}  z_d={:+.1}  z_ΔH={:+.1}  H_b={:.2}{}{}",
                    m.name, m.divergence, m.entropy_delta, m.z_divergence, m.z_entropy_delta,
                    m.baseline_entropy, pct_str, trend_str
                );
            } else {
                println!("    {:>10}: (skipped — not applicable)", m.name);
            }
        }
    }

    let mut sources: Vec<_> = report.source_scores.iter().collect();
    sources.sort_by_key(|(id, _)| id.0);

    for (id, src) in &sources {
        println!("  source {:>3}: divergence {:.2}", id.0, src.divergence);
    }

    if !report.missing_sources.baseline_only.is_empty() {
        let ids: Vec<_> = report.missing_sources.baseline_only.iter().map(|id| id.0.to_string()).collect();
        println!("  missing in test: {}", ids.join(", "));
    }
    if !report.missing_sources.test_only.is_empty() {
        let ids: Vec<_> = report.missing_sources.test_only.iter().map(|id| id.0.to_string()).collect();
        println!("  new in test: {}", ids.join(", "));
    }
}
