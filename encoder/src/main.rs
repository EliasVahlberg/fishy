use clap::{Parser, Subcommand};
use encoder::{build_dictionary, build_drain_tree, encode, Dictionary, DrainTree, LogInput};
use analysis::SourceId;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "encoder", about = "Log tokenizer and encoder for fishy")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build a Drain tree and template dictionary from a directory of log files.
    BuildDict {
        /// Directory containing log files (one file = one source).
        dir: PathBuf,
        /// Output dictionary path (drain.json written alongside).
        #[arg(short, long, default_value = "dict.json")]
        output: PathBuf,
        /// Drain similarity threshold (0.0–1.0).
        #[arg(long, default_value = "0.5")]
        sim_threshold: f64,
        /// Maximum children per Drain tree node.
        #[arg(long, default_value = "100")]
        max_children: usize,
    },
    /// Encode a directory of log files into a fishy LogCollection.
    Encode {
        /// Directory containing log files.
        dir: PathBuf,
        /// Dictionary built from the baseline.
        #[arg(short, long)]
        dict: PathBuf,
        /// Output directory for the encoded collection.
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::BuildDict { dir, output, sim_threshold, max_children } => {
            let inputs = collect_inputs(&dir);
            let tree = build_drain_tree(&inputs, sim_threshold, max_children);
            let dict = build_dictionary(&inputs, &tree);

            // Save drain.json alongside dict.json
            let drain_path = output.with_file_name("drain.json");
            tree.save(&drain_path)
                .unwrap_or_else(|e| { eprintln!("error saving drain tree: {e}"); std::process::exit(1); });
            dict.save(&output)
                .unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
            println!("drain: {} clusters → {}", tree.num_clusters(), drain_path.display());
            println!("dictionary: {} templates → {}", dict.len(), output.display());
        }
        Cmd::Encode { dir, dict: dict_path, output } => {
            let drain_path = dict_path.with_file_name("drain.json");
            let tree = DrainTree::load(&drain_path)
                .unwrap_or_else(|e| { eprintln!("error loading drain tree: {e}"); std::process::exit(1); });
            let dict = Dictionary::load(&dict_path)
                .unwrap_or_else(|e| { eprintln!("error loading dict: {e}"); std::process::exit(1); });
            let inputs = collect_inputs(&dir);
            let collection = encode(&inputs, &tree, &dict);
            std::fs::create_dir_all(&output).unwrap();
            let meta = serde_json::json!({
                "start_time": collection.metadata.start_time,
                "end_time": collection.metadata.end_time,
            });
            std::fs::write(output.join("meta.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
            for (id, stream) in &collection.sources {
                let path = output.join(format!("{}.json", id.0));
                std::fs::write(&path, serde_json::to_string_pretty(stream).unwrap()).unwrap();
            }
            println!("encoded {} sources → {}", collection.sources.len(), output.display());
        }
    }
}

/// Collect log files from a directory, grouping rotated files (auth.log, auth.log.1, …)
/// into a single LogInput per base name. SourceIds assigned by sorted base name order.
fn collect_inputs(dir: &PathBuf) -> Vec<LogInput> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| { eprintln!("error reading {}: {e}", dir.display()); std::process::exit(1); })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();

    let mut groups: std::collections::BTreeMap<String, Vec<PathBuf>> = std::collections::BTreeMap::new();
    for path in paths {
        let base = log_base_name(&path);
        groups.entry(base).or_default().push(path);
    }

    groups
        .into_values()
        .enumerate()
        .map(|(i, mut group_paths)| {
            group_paths.sort_by(|a, b| {
                let a_n = rotation_number(a);
                let b_n = rotation_number(b);
                b_n.cmp(&a_n)
            });
            LogInput { source_id: SourceId(i as u32), paths: group_paths }
        })
        .collect()
}

fn log_base_name(path: &std::path::Path) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if let Some(pos) = name.rfind('.') {
        if name[pos + 1..].chars().all(|c| c.is_ascii_digit()) {
            return name[..pos].to_string();
        }
    }
    name.to_string()
}

fn rotation_number(path: &std::path::Path) -> u32 {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if let Some(pos) = name.rfind('.') {
        if let Ok(n) = name[pos + 1..].parse::<u32>() {
            return n;
        }
    }
    0
}
