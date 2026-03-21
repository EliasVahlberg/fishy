use analysis::SourceId;
use std::path::PathBuf;

/// Supported log formats.
#[derive(Clone, Debug)]
pub enum LogFormat {
    /// nginx/apache combined access log.
    NginxAccess,
    /// RFC 3164 / RFC 5424 syslog.
    Syslog,
    /// One JSON object per line; specify the message field name.
    Json { message_field: String, timestamp_field: String },
    /// User-supplied regex with named captures `(?P<timestamp>...)` and `(?P<message>...)`.
    Custom { pattern: String },
    /// Blue Gene/L supercomputer log (LogHub BGL dataset).
    /// Format: LABEL UNIX_TS DATE NODE FULL_TS NODE COMPONENT SUBSYSTEM SEVERITY MESSAGE
    Bgl,
}

/// A single log file to be encoded.
#[derive(Clone, Debug)]
pub struct LogInput {
    pub source_id: SourceId,
    pub path: PathBuf,
    pub format: LogFormat,
}

/// Parse a timestamp string into Unix seconds.
/// Returns `None` if parsing fails — the event is still included with ts=0.
pub fn parse_timestamp(ts: &str, format: &LogFormat) -> Option<u64> {
    match format {
        LogFormat::NginxAccess => parse_nginx_ts(ts),
        LogFormat::Syslog => parse_syslog_ts(ts),
        // BGL, Json, Custom all carry Unix seconds as the timestamp string.
        LogFormat::Bgl | LogFormat::Json { .. } | LogFormat::Custom { .. } => {
            ts.parse::<u64>().ok()
        }
    }
}

// ---------------------------------------------------------------------------
// Timestamp parsers
// ---------------------------------------------------------------------------

/// nginx: `10/Oct/2000:13:55:36 -0700`
fn parse_nginx_ts(ts: &str) -> Option<u64> {
    // Minimal: parse day/month/year hour:min:sec, ignore timezone.
    let parts: Vec<&str> = ts.splitn(2, ':').collect();
    if parts.len() < 2 { return None; }
    let date_part = parts[0]; // "10/Oct/2000"
    let time_part = parts[1]; // "13:55:36 -0700"

    let dp: Vec<&str> = date_part.split('/').collect();
    if dp.len() != 3 { return None; }
    let day: u64 = dp[0].parse().ok()?;
    let month = month_num(dp[1])?;
    let year: u64 = dp[2].parse().ok()?;

    let tp: Vec<&str> = time_part.split_whitespace().next()?.split(':').collect();
    if tp.len() < 3 { return None; }
    let h: u64 = tp[0].parse().ok()?;
    let m: u64 = tp[1].parse().ok()?;
    let s: u64 = tp[2].parse().ok()?;

    // Rough Unix seconds (ignores leap seconds and timezone).
    let days_since_epoch = days_from_ymd(year, month, day)?;
    Some(days_since_epoch * 86400 + h * 3600 + m * 60 + s)
}

/// syslog: `Jan 10 13:55:36` (no year — assume current year)
fn parse_syslog_ts(ts: &str) -> Option<u64> {
    let parts: Vec<&str> = ts.split_whitespace().collect();
    if parts.len() < 3 { return None; }
    let month = month_num(parts[0])?;
    let day: u64 = parts[1].parse().ok()?;
    let tp: Vec<&str> = parts[2].split(':').collect();
    if tp.len() < 3 { return None; }
    let h: u64 = tp[0].parse().ok()?;
    let m: u64 = tp[1].parse().ok()?;
    let s: u64 = tp[2].parse().ok()?;
    let year = 2025u64; // fixed; relative timestamps make the exact year irrelevant
    let days = days_from_ymd(year, month, day)?;
    Some(days * 86400 + h * 3600 + m * 60 + s)
}

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

/// Days since Unix epoch (1970-01-01) for a given date. Gregorian calendar.
fn days_from_ymd(y: u64, m: u64, d: u64) -> Option<u64> {
    if y < 1970 || m < 1 || m > 12 || d < 1 { return None; }
    let months = [0u64, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = if m > 2 && (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) { 1 } else { 0 };
    let year_days = (y - 1970) * 365 + (y - 1969) / 4 - (y - 1901) / 100 + (y - 1601) / 400;
    Some(year_days + months[(m - 1) as usize] + leap + d - 1)
}
