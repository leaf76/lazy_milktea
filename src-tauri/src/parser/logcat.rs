use crate::types::LogRow;
use regex::Regex;
use once_cell::sync::Lazy;

/// Logcat line regex (threadtime format)
/// This regex is shared across parser and index modules
pub static LOGCAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<date>\d{2}-\d{2})\s+(?P<time>\d{2}:\d{2}:\d{2}\.\d{3})\s+(?P<pid>\d+)\s+(?P<tid>\d+)\s+(?P<level>[VDIWEF])\s+(?P<tag>[^:]+):\s(?P<msg>.*)$"
    ).unwrap()
});

/// Logcat line regex with multiline support (for bulk text parsing)
pub static LOGCAT_RE_MULTILINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(?P<date>\d{2}-\d{2})\s+(?P<time>\d{2}:\d{2}:\d{2}\.\d{3})\s+(?P<pid>\d+)\s+(?P<tid>\d+)\s+(?P<level>[VDIWEF])\s+(?P<tag>[^:]+):\s(?P<msg>.*)$"
    ).unwrap()
});

/// Parse a single logcat line into LogRow
pub fn parse_logcat_line(line: &str) -> Option<LogRow> {
    let caps = LOGCAT_RE.captures(line)?;

    Some(LogRow {
        ts: format!("{} {}", &caps["date"], &caps["time"]),
        ts_iso: None, // To be filled by caller with proper time anchor
        level: caps["level"].to_string(),
        tag: caps["tag"].to_string(),
        pid: caps["pid"].parse().unwrap_or_default(),
        tid: caps["tid"].parse().unwrap_or_default(),
        msg: caps["msg"].to_string(),
    })
}

/// Check if a line looks like a logcat entry
pub fn is_logcat_line(line: &str) -> bool {
    LOGCAT_RE.is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_logcat_line() {
        let line = "08-24 14:22:33.123  1234  5678 E ActivityManager: ANR in com.foo";
        let row = parse_logcat_line(line).unwrap();

        assert_eq!(row.ts, "08-24 14:22:33.123");
        assert_eq!(row.level, "E");
        assert_eq!(row.tag, "ActivityManager");
        assert_eq!(row.pid, 1234);
        assert_eq!(row.tid, 5678);
        assert_eq!(row.msg, "ANR in com.foo");
    }

    #[test]
    fn test_parse_logcat_line_with_spaces() {
        let line = "08-24 14:22:33.123  1234  5678 I My Tag: hello world";
        let row = parse_logcat_line(line).unwrap();

        assert_eq!(row.tag, "My Tag");
        assert_eq!(row.msg, "hello world");
    }

    #[test]
    fn test_invalid_line() {
        assert!(parse_logcat_line("not a logcat line").is_none());
        assert!(parse_logcat_line("").is_none());
    }

    #[test]
    fn test_is_logcat_line() {
        assert!(is_logcat_line("08-24 14:22:33.123  1234  5678 E Tag: msg"));
        assert!(!is_logcat_line("random text"));
    }
}
