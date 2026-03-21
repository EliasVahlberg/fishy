use clap::{Parser, Subcommand};
use encoder::{build_dictionary, encode, Dictionary, LogFormat, LogInput};
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
    /// Build a template dictionary from a directory of log files.
    BuildDict {
        /// Directory containing log files (one file = one source).
        dir: PathBuf,
        /// Output dictionary path.
        #[arg(short, long, default_value = "dict.json")]
        output: PathBuf,
        /// Log format: nginx | syslog | json | custom:<regex>
        #[arg(short, long, default_value = "syslog")]
        format: String,
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
        /// Log format: nginx | syslog | json | custom:<regex>
        #[arg(short, long, default_value = "syslog")]
        format: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::BuildDict { dir, output, format } => {
            let inputs = collect_inputs(&dir, &format);
            let dict = build_dictionary(&inputs);
            dict.save(&output).unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
            println!("dictionary: {} templates → {}", dict.len(), output.display());
        }
        Cmd::Encode { dir, dict: dict_path, output, format } => {
            let dict = Dictionary::load(&dict_path)
                .unwrap_or_else(|e| { eprintln!("error loading dict: {e}"); std::process::exit(1); });
            let inputs = collect_inputs(&dir, &format);
            let collection = encode(&inputs, &dict);
            std::fs::create_dir_all(&output).unwrap();
            // Write meta.json
            let meta = serde_json::json!({
                "start_time": collection.metadata.start_time,
                "end_time": collection.metadata.end_time,
            });
            std::fs::write(output.join("meta.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
            // Write per-source files
            for (id, stream) in &collection.sources {
                let path = output.join(format!("{}.json", id.0));
                std::fs::write(&path, serde_json::to_string_pretty(stream).unwrap()).unwrap();
            }
            println!("encoded {} sources → {}", collection.sources.len(), output.display());
        }
    }
}

/// Collect log files from a directory, assigning SourceIds by sorted filename order.
fn collect_inputs(dir: &PathBuf, format_str: &str) -> Vec<LogInput> {
    let format = parse_format(format_str);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| { eprintln!("error reading {}: {e}", dir.display()); std::process::exit(1); })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| LogInput { source_id: SourceId(i as u32), path, format: format.clone() })
        .collect()
}

fn parse_format(s: &str) -> LogFormat {
    match s {
        "nginx" => LogFormat::NginxAccess,
        "syslog" => LogFormat::Syslog,
        "bgl" => LogFormat::Bgl,
        s if s.starts_with("json:") => {
            let parts: Vec<&str> = s[5..].splitn(2, ',').collect();
            LogFormat::Json {
                message_field: parts.first().copied().unwrap_or("message").to_string(),
                timestamp_field: parts.get(1).copied().unwrap_or("timestamp").to_string(),
            }
        }
        s if s.starts_with("custom:") => LogFormat::Custom { pattern: s[7..].to_string() },
        _ => LogFormat::Syslog,
    }
}
