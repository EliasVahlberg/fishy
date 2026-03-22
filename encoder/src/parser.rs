use analysis::SourceId;
use regex::Regex;
use std::path::PathBuf;
use std::sync::OnceLock;

/// A single log source, potentially spanning multiple rotated files.
#[derive(Clone, Debug)]
pub struct LogInput {
    pub source_id: SourceId,
    pub paths: Vec<PathBuf>,
}

/// Try to extract a Unix-seconds timestamp from the beginning of a line.
/// Tries common patterns in order: ISO 8601, syslog, nginx/apache access,
/// apache error, unix seconds. Returns (timestamp_unix_secs, rest_of_line).
pub fn extract_timestamp(line: &str) -> (Option<u64>, &str) {
    // JSON lines — try timestamp field
    if line.starts_with('{') {
        return extract_json_ts(line);
    }

    // Try each pattern against the line start
    for extractor in EXTRACTORS.iter() {
        if let Some((ts, rest)) = extractor(line) {
            return (Some(ts), rest);
        }
    }

    (None, line)
}

type TsExtractor = fn(&str) -> Option<(u64, &str)>;

const EXTRACTORS: &[TsExtractor] = &[
    extract_iso8601,
    extract_syslog_ts,
    extract_nginx_ts,
    extract_apache_error_ts,
    extract_unix_seconds,
];

/// ISO 8601: `2022-01-21T03:01:00+01:00` or `2022-01-21 03:01:00`
fn extract_iso8601(line: &str) -> Option<(u64, &str)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2})(?:\.\d+)?(?:[+-]\d{2}:?\d{2}|Z)?\s*").unwrap()
    });
    let caps = re.captures(line)?;
    let ts_str = &caps[1];
    let rest = &line[caps[0].len()..];
    // Parse: YYYY-MM-DDxHH:MM:SS
    let y: u64 = ts_str[0..4].parse().ok()?;
    let m: u64 = ts_str[5..7].parse().ok()?;
    let d: u64 = ts_str[8..10].parse().ok()?;
    let h: u64 = ts_str[11..13].parse().ok()?;
    let min: u64 = ts_str[14..16].parse().ok()?;
    let s: u64 = ts_str[17..19].parse().ok()?;
    let days = days_from_ymd(y, m, d)?;
    Some((days * 86400 + h * 3600 + min * 60 + s, rest))
}

/// Syslog: `Jan 10 13:55:36 hostname ...`
fn extract_syslog_ts(line: &str) -> Option<(u64, &str)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(\w{3})\s+(\d{1,2})\s+(\d{2}):(\d{2}):(\d{2})\s+").unwrap()
    });
    let caps = re.captures(line)?;
    let month = month_num(&caps[1])?;
    let day: u64 = caps[2].parse().ok()?;
    let h: u64 = caps[3].parse().ok()?;
    let min: u64 = caps[4].parse().ok()?;
    let s: u64 = caps[5].parse().ok()?;
    let year = 2025u64;
    let days = days_from_ymd(year, month, day)?;
    let rest = &line[caps[0].len()..];
    Some((days * 86400 + h * 3600 + min * 60 + s, rest))
}

/// nginx/apache access: `... [10/Oct/2000:13:55:36 -0700] ...`
/// We look for the bracketed timestamp anywhere in the line.
fn extract_nginx_ts(line: &str) -> Option<(u64, &str)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"\[(\d{2})/(\w{3})/(\d{4}):(\d{2}):(\d{2}):(\d{2})\s+[^\]]*\]").unwrap()
    });
    let caps = re.captures(line)?;
    let day: u64 = caps[1].parse().ok()?;
    let month = month_num(&caps[2])?;
    let year: u64 = caps[3].parse().ok()?;
    let h: u64 = caps[4].parse().ok()?;
    let min: u64 = caps[5].parse().ok()?;
    let s: u64 = caps[6].parse().ok()?;
    let days = days_from_ymd(year, month, day)?;
    Some((days * 86400 + h * 3600 + min * 60 + s, line))
}

/// Apache error: `[Wed Oct 11 14:32:52.123456 2000] ...`
fn extract_apache_error_ts(line: &str) -> Option<(u64, &str)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^\[\w+ (\w+)\s+(\d{1,2})\s+(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?\s+(\d{4})\]\s*").unwrap()
    });
    let caps = re.captures(line)?;
    let month = month_num(&caps[1])?;
    let day: u64 = caps[2].parse().ok()?;
    let h: u64 = caps[3].parse().ok()?;
    let min: u64 = caps[4].parse().ok()?;
    let s: u64 = caps[5].parse().ok()?;
    let year: u64 = caps[6].parse().ok()?;
    let days = days_from_ymd(year, month, day)?;
    let rest = &line[caps[0].len()..];
    Some((days * 86400 + h * 3600 + min * 60 + s, rest))
}

/// Bare unix seconds at start of line (e.g. BGL: `- 1117838570 ...`)
fn extract_unix_seconds(line: &str) -> Option<(u64, &str)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(?:\S+\s+)?(\d{9,10})\s+").unwrap()
    });
    let caps = re.captures(line)?;
    let ts: u64 = caps[1].parse().ok()?;
    // Sanity: must be a plausible Unix timestamp (2001-2030)
    if ts < 978_307_200 || ts > 1_893_456_000 { return None; }
    let rest = &line[caps[0].len()..];
    Some((ts, rest))
}

/// JSON: try common timestamp fields
fn extract_json_ts(line: &str) -> (Option<u64>, &str) {
    let v: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return (None, line),
    };
    for field in &["timestamp", "@timestamp", "time", "ts", "datetime"] {
        if let Some(val) = v.get(field) {
            if let Some(n) = val.as_u64() {
                return (Some(n), line);
            }
            if let Some(s) = val.as_str() {
                if let Some((ts, _)) = extract_iso8601(s) {
                    return (Some(ts), line);
                }
                if let Ok(n) = s.parse::<u64>() {
                    return (Some(n), line);
                }
            }
        }
    }
    (None, line)
}

// ---------------------------------------------------------------------------
// Calendar helpers
// ---------------------------------------------------------------------------

fn month_num(s: &str) -> Option<u64> {
    match s {
        "Jan" | "January"   => Some(1),  "Feb" | "February"  => Some(2),
        "Mar" | "March"     => Some(3),  "Apr" | "April"     => Some(4),
        "May"               => Some(5),  "Jun" | "June"      => Some(6),
        "Jul" | "July"      => Some(7),  "Aug" | "August"    => Some(8),
        "Sep" | "September" => Some(9),  "Oct" | "October"   => Some(10),
        "Nov" | "November"  => Some(11), "Dec" | "December"  => Some(12),
        _ => None,
    }
}

fn days_from_ymd(y: u64, m: u64, d: u64) -> Option<u64> {
    if y < 1970 || m < 1 || m > 12 || d < 1 { return None; }
    let months = [0u64, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = if m > 2 && (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) { 1 } else { 0 };
    let year_days = (y - 1970) * 365 + (y - 1969) / 4 - (y - 1901) / 100 + (y - 1601) / 400;
    Some(year_days + months[(m - 1) as usize] + leap + d - 1)
}
