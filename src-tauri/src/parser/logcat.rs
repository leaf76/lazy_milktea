use crate::types::LogRow;
use regex::Regex;
use once_cell::sync::Lazy;

/// Logcat line regex (threadtime format with optional UID)
/// Supports both formats:
/// - Standard: MM-DD HH:MM:SS.mmm  PID  TID LEVEL Tag: msg
/// - With UID: MM-DD HH:MM:SS.mmm  UID  PID  TID LEVEL Tag: msg (from -v uid flag)
/// UID can be numeric (1000) or text (root, wifi, etc.)
pub static LOGCAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<date>\d{2}-\d{2})\s+(?P<time>\d{2}:\d{2}:\d{2}\.\d{3})\s+(?:\S+\s+)?(?P<pid>\d+)\s+(?P<tid>\d+)\s+(?P<level>[VDIWEF])\s+(?P<tag>[^:]+):\s(?P<msg>.*)$"
    ).unwrap()
});

/// Logcat line regex with multiline support (for bulk text parsing)
/// Supports both standard and UID formats
pub static LOGCAT_RE_MULTILINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(?P<date>\d{2}-\d{2})\s+(?P<time>\d{2}:\d{2}:\d{2}\.\d{3})\s+(?:\S+\s+)?(?P<pid>\d+)\s+(?P<tid>\d+)\s+(?P<level>[VDIWEF])\s+(?P<tag>[^:]+):\s(?P<msg>.*)$"
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

    #[test]
    fn test_parse_logcat_line_with_numeric_uid() {
        // Format from bugreport with -v uid flag (numeric UID)
        let line = "12-07 02:19:18.876  1000  1675  1694 W ProcessStats: Tracking association";
        let row = parse_logcat_line(line).unwrap();

        assert_eq!(row.ts, "12-07 02:19:18.876");
        assert_eq!(row.level, "W");
        assert_eq!(row.tag, "ProcessStats");
        assert_eq!(row.pid, 1675);
        assert_eq!(row.tid, 1694);
        assert_eq!(row.msg, "Tracking association");
    }

    #[test]
    fn test_parse_logcat_line_with_text_uid() {
        // Format from bugreport with -v uid flag (text UID like root, wifi)
        let line = "12-07 02:22:40.233  wifi  1404  1475 I vendor.google.wifi_ext: Setting SAR";
        let row = parse_logcat_line(line).unwrap();

        assert_eq!(row.ts, "12-07 02:22:40.233");
        assert_eq!(row.level, "I");
        assert_eq!(row.tag, "vendor.google.wifi_ext");
        assert_eq!(row.pid, 1404);
        assert_eq!(row.tid, 1475);
        assert_eq!(row.msg, "Setting SAR");
    }

    #[test]
    fn test_parse_logcat_line_without_uid() {
        // Standard format without UID
        let line = "12-08 00:40:03.963 19264 19264 I apexd   : Populating APEX database";
        let row = parse_logcat_line(line).unwrap();

        assert_eq!(row.ts, "12-08 00:40:03.963");
        assert_eq!(row.level, "I");
        assert_eq!(row.pid, 19264);
        assert_eq!(row.tid, 19264);
        assert_eq!(row.msg, "Populating APEX database");
    }
}
