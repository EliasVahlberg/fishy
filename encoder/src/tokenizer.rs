use crate::parser::LogFormat;
use regex::Regex;
use std::sync::OnceLock;

/// Extract a normalised template string from a log line.
/// Returns `None` if the line should be skipped (empty, comment).
pub fn extract_template(line: &str, format: &LogFormat) -> Option<String> {
    extract_template_and_ts(line, format).map(|(t, _)| t)
}

/// Extract both the template string and the raw timestamp string from a log line.
pub fn extract_template_and_ts(line: &str, format: &LogFormat) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    match format {
        LogFormat::NginxAccess => parse_nginx(line),
        LogFormat::Syslog => parse_syslog(line),
        LogFormat::Json { message_field, timestamp_field } => {
            parse_json_line(line, message_field, timestamp_field)
        }
        LogFormat::Custom { pattern } => parse_custom(line, pattern),
    }
}

// ---------------------------------------------------------------------------
// Format-specific parsers
// ---------------------------------------------------------------------------

/// nginx combined: `IP - - [timestamp] "METHOD /path HTTP/x.x" status bytes ...`
fn parse_nginx(line: &str) -> Option<(String, String)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"^\S+ \S+ \S+ \[([^\]]+)\] "([A-Z]+) ([^ "]+)[^"]*" (\d+)"#).unwrap()
    });
    let caps = re.captures(line)?;
    let ts = caps[1].to_string();
    let method = &caps[2];
    let path = normalise_path(&caps[3]);
    let status = &caps[4];
    Some((format!("{method} {path} {status}"), ts))
}

/// syslog: `Mon DD HH:MM:SS host process[pid]: message`
fn parse_syslog(line: &str) -> Option<(String, String)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(\w{3}\s+\d+\s+\d+:\d+:\d+)\s+\S+\s+(\S+?)(?:\[\d+\])?:\s+(.+)$").unwrap()
    });
    let caps = re.captures(line)?;
    let ts = caps[1].to_string();
    let process = &caps[2];
    let message = normalise_message(&caps[3]);
    Some((format!("{process}: {message}"), ts))
}

/// JSON log: one object per line, extract message and timestamp fields.
fn parse_json_line(line: &str, msg_field: &str, ts_field: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let msg = v.get(msg_field)?.as_str()?;
    let ts = v.get(ts_field).and_then(|t| t.as_str().map(String::from)
        .or_else(|| t.as_u64().map(|n| n.to_string())))?;
    Some((normalise_message(msg), ts))
}

/// Custom regex with named captures `timestamp` and `message`.
fn parse_custom(line: &str, pattern: &str) -> Option<(String, String)> {
    let re = Regex::new(pattern).ok()?;
    let caps = re.captures(line)?;
    let ts = caps.name("timestamp")?.as_str().to_string();
    let msg = caps.name("message")?.as_str();
    Some((normalise_message(msg), ts))
}

// ---------------------------------------------------------------------------
// Normalisation — replace variable tokens with placeholders
// ---------------------------------------------------------------------------

/// Normalise a URL path: replace numeric segments and UUIDs with `<id>`.
fn normalise_path(path: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?x)
            /\d+(?:/|$)                          # numeric path segment
            | [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}  # UUID
            | \?.*$                              # query string
        ").unwrap()
    });
    re.replace_all(path, "/<id>").into_owned()
}

/// Normalise a free-text message: replace numbers, IPs, hex strings, paths.
fn normalise_message(msg: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?x)
            \b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b  # IPv4
            | \b[0-9a-fA-F]{8,}\b                    # hex string / hash
            | \b\d+\b                                 # bare number
            | /[\w./\-]+                              # file path
        ").unwrap()
    });
    re.replace_all(msg, "<v>").into_owned()
}
