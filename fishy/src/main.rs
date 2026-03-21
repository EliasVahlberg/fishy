use clap::Parser;
use fishy::{load_collection, AnomalyReport, DetectConfig};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fishy", about = "Multi-source anomaly detection")]
struct Cli {
    /// Baseline collection directory
    #[arg(short, long)]
    baseline: PathBuf,

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
}

fn main() {
    let cli = Cli::parse();

    let baseline = load_collection(&cli.baseline).unwrap_or_else(|e| {
        eprintln!("error loading baseline: {e}");
        std::process::exit(1);
    });
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
        ..DetectConfig::default()
    };

    match fishy::detect(&baseline, &test, &config) {
        Ok(report) => {
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!("{}", report.verdict);
                if cli.verbose {
                    print_verbose(&report);
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn print_verbose(report: &AnomalyReport) {
    if report.meta_conflict > 0.05 {
        println!("  meta-conflict: {:.2} (methods disagree)", report.meta_conflict);
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
