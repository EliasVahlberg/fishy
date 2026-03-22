use crate::parser::LogFormat;
use regex::Regex;
use std::sync::OnceLock;

/// Extract a normalised template string and optional raw timestamp from a log line.
///
/// Returns `None` if the line should be skipped entirely (empty, comment, no parseable content).
/// Returns `Some((template, None))` when the line has content but no timestamp — the caller
/// should apply the last seen timestamp (sticky-timestamp model).
pub fn extract_template_and_ts(line: &str, format: &LogFormat) -> Option<(String, Option<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    match format {
        LogFormat::NginxAccess | LogFormat::ApacheAccess => parse_nginx(line),
        LogFormat::ApacheError => parse_apache_error(line),
        LogFormat::Syslog => parse_syslog(line),
        LogFormat::Json { message_field, timestamp_field } => {
            parse_json_line(line, message_field, timestamp_field)
        }
        LogFormat::Custom { pattern } => parse_custom(line, pattern),
        LogFormat::Bgl => parse_bgl(line),
    }
}

// ---------------------------------------------------------------------------
// Format-specific parsers
// ---------------------------------------------------------------------------

fn parse_nginx(line: &str) -> Option<(String, Option<String>)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"^\S+ \S+ \S+ \[([^\]]+)\] "([A-Z]+) ([^ "]+)[^"]*" (\d+)"#).unwrap()
    });
    let caps = re.captures(line)?;
    let ts = caps[1].to_string();
    let method = &caps[2];
    let path = normalise_path(&caps[3]);
    let status = &caps[4];
    Some((format!("{method} {path} {status}"), Some(ts)))
}

fn parse_syslog(line: &str) -> Option<(String, Option<String>)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(\w{3}\s+\d+\s+\d+:\d+:\d+)\s+\S+\s+(\S+?)(?:\[\d+\])?:\s+(.+)$").unwrap()
    });
    let caps = re.captures(line)?;
    let ts = caps[1].to_string();
    let process = &caps[2];
    let message = normalise_message(&caps[3]);
    Some((format!("{process}: {message}"), Some(ts)))
}

/// Apache 2.4 error log: `[Wed Oct 11 14:32:52.123456 2000] [error] [pid 1234] [client ...] msg`
fn parse_apache_error(line: &str) -> Option<(String, Option<String>)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^\[(\w+ \w+ +\d+ [\d:.]+\s+\d+)\] \[([^\]]+)\] (?:\[pid \d+\] )?(?:\[client [^\]]+\] )?(.+)$").unwrap()
    });
    let caps = re.captures(line)?;
    let ts = caps[1].to_string();
    let level = &caps[2];
    let message = normalise_message(&caps[3]);
    Some((format!("{level}: {message}"), Some(ts)))
}

fn parse_json_line(line: &str, msg_field: &str, ts_field: &str) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let msg = json_get_path(&v, msg_field)?.as_str()?;
    let ts = json_get_path(&v, ts_field).and_then(|t| {
        t.as_str().map(String::from).or_else(|| t.as_u64().map(|n| n.to_string()))
    });
    Some((normalise_message(msg), ts))
}

/// Traverse a dotted field path through a JSON value (e.g. `alert.signature`).
fn json_get_path<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').try_fold(v, |acc, key| acc.get(key))
}

fn parse_custom(line: &str, pattern: &str) -> Option<(String, Option<String>)> {
    let re = Regex::new(pattern).ok()?;
    let caps = re.captures(line)?;
    let ts = caps.name("timestamp").map(|m| m.as_str().to_string());
    let msg = caps.name("message")?.as_str();
    Some((normalise_message(msg), ts))
}

/// BGL: `LABEL UNIX_TS DATE NODE FULL_TS NODE COMPONENT SUBSYSTEM SEVERITY MESSAGE`
fn parse_bgl(line: &str) -> Option<(String, Option<String>)> {
    let mut parts = line.splitn(10, char::is_whitespace).filter(|s| !s.is_empty());
    let _label     = parts.next()?;
    let unix_ts    = parts.next()?;
    let _date      = parts.next()?;
    let _node      = parts.next()?;
    let _full_ts   = parts.next()?;
    let _node2     = parts.next()?;
    let component  = parts.next()?;
    let subsystem  = parts.next()?;
    let severity   = parts.next()?;
    let message    = parts.next().unwrap_or("");
    let template = format!("{component} {subsystem} {severity}: {}", normalise_message(message));
    Some((template, Some(unix_ts.to_string())))
}

// ---------------------------------------------------------------------------
// Normalisation
// ---------------------------------------------------------------------------

fn normalise_path(path: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?x)
            /\d+(?:/|$)
            | [0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}
            | \?.*$
        ").unwrap()
    });
    re.replace_all(path, "/<id>").into_owned()
}

fn normalise_message(msg: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?x)
            \b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b
            | \b[0-9a-fA-F]{8,}\b
            | \b\d+\b
            | /[\w./\-]+
        ").unwrap()
    });
    re.replace_all(msg, "<v>").into_owned()
}
