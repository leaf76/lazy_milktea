use chrono::{Local, Datelike, NaiveDate};
use chrono_tz::Tz;
use regex::Regex;
use once_cell::sync::Lazy;

/// Time anchor derived from bugreport content
#[derive(Debug, Clone)]
pub struct TimeAnchor {
    pub tz: Tz,
    pub year: i32,
    pub report_date: Option<NaiveDate>,
}

static TZ_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*persist\.sys\.timezone\s*=\s*(?P<tz>\S+)\s*$").unwrap()
});

static DUMPSTATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"dumpstate:\s*(\d{4})-(\d{2})-(\d{2})").unwrap()
});

static BUILD_DATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\d{2})(\d{2})(\d{2})\.(\d{3})").unwrap()
});

/// Extract timezone from bugreport content
fn extract_timezone(text: &str) -> Option<Tz> {
    TZ_RE.captures(text)
        .and_then(|cap| cap["tz"].parse::<Tz>().ok())
}

/// Extract report date from dumpstate header or build fingerprint
fn extract_report_date(text: &str) -> Option<NaiveDate> {
    // Priority 1: dumpstate timestamp (most reliable)
    // Format: "== dumpstate: 2024-08-24 14:22:33"
    if let Some(cap) = DUMPSTATE_RE.captures(text) {
        let y: i32 = cap[1].parse().ok()?;
        let m: u32 = cap[2].parse().ok()?;
        let d: u32 = cap[3].parse().ok()?;
        return NaiveDate::from_ymd_opt(y, m, d);
    }

    // Priority 2: Build fingerprint date
    // Format: "TQ3A.230605.012" -> 2023-06-05
    if let Some(cap) = BUILD_DATE_RE.captures(text) {
        let y: i32 = 2000 + cap[1].parse::<i32>().ok()?;
        let m: u32 = cap[2].parse().ok()?;
        let d: u32 = cap[3].parse().ok()?;
        return NaiveDate::from_ymd_opt(y, m, d);
    }

    None
}

/// Derive time anchor from bugreport content
/// This includes timezone and reference year for timestamp conversion
pub fn derive_time_anchor(text: &str) -> TimeAnchor {
    let tz = extract_timezone(text).unwrap_or(chrono_tz::UTC);
    let report_date = extract_report_date(text);
    let year = report_date
        .map(|d| d.year())
        .unwrap_or_else(|| Local::now().year());

    TimeAnchor { tz, year, report_date }
}

/// Infer the most likely year for a given month/day based on reference date
pub fn infer_year(mon: u32, day: u32, reference: NaiveDate) -> i32 {
    let ref_year = reference.year();

    // Try current year, previous year, and next year
    let candidates = [ref_year, ref_year - 1, ref_year + 1];

    candidates
        .into_iter()
        .filter_map(|y| NaiveDate::from_ymd_opt(y, mon, day))
        .min_by_key(|d| d.signed_duration_since(reference).num_days().abs())
        .map(|d| d.year())
        .unwrap_or(ref_year)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_timezone() {
        let text = "persist.sys.timezone=Asia/Taipei\nother=value";
        let tz = extract_timezone(text).unwrap();
        assert_eq!(tz.to_string(), "Asia/Taipei");
    }

    #[test]
    fn test_extract_timezone_utc_fallback() {
        let text = "no timezone here";
        assert!(extract_timezone(text).is_none());
    }

    #[test]
    fn test_extract_report_date_dumpstate() {
        let text = "== dumpstate: 2024-08-24 14:22:33\nother content";
        let date = extract_report_date(text).unwrap();
        assert_eq!(date, NaiveDate::from_ymd_opt(2024, 8, 24).unwrap());
    }

    #[test]
    fn test_infer_year_same_year() {
        let reference = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        assert_eq!(infer_year(8, 24, reference), 2024);
    }

    #[test]
    fn test_infer_year_cross_year() {
        // Reference is Jan 2024, log is from Dec -> should be Dec 2023
        let reference = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap();
        assert_eq!(infer_year(12, 25, reference), 2023);
    }
}
