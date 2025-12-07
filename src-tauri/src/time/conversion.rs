use crate::error::{LogcatError, Result};
use crate::time::anchor::{TimeAnchor, infer_year};
use chrono::{DateTime, Local, Utc, NaiveDate, NaiveTime, NaiveDateTime, TimeZone, LocalResult};
use anyhow::anyhow;

/// Convert threadtime format timestamp to ISO 8601 (UTC)
/// Handles DST boundaries safely
///
/// Input format: "MM-DD HH:MM:SS.mmm" (e.g., "08-24 14:22:33.123")
/// Output format: RFC3339 UTC (e.g., "2024-08-24T06:22:33.123+00:00")
pub fn to_iso_safe(ts_threadtime: &str, anchor: &TimeAnchor) -> Result<String> {
    let naive = parse_threadtime(ts_threadtime, anchor)?;

    match anchor.tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => {
            Ok(dt.with_timezone(&Utc).to_rfc3339())
        }
        LocalResult::Ambiguous(earlier, _later) => {
            // DST ends: same local time maps to two UTC times
            // Strategy: use earlier interpretation (conservative)
            log::debug!(
                "Ambiguous time {} in {}, using earlier interpretation",
                ts_threadtime, anchor.tz
            );
            Ok(earlier.with_timezone(&Utc).to_rfc3339())
        }
        LocalResult::None => {
            // DST starts: certain local times don't exist
            // Strategy: shift forward by 1 hour
            let adjusted = naive + chrono::Duration::hours(1);
            match anchor.tz.from_local_datetime(&adjusted) {
                LocalResult::Single(dt) => {
                    log::debug!(
                        "Non-existent time {} in {}, adjusted to {}",
                        ts_threadtime, anchor.tz, dt
                    );
                    Ok(dt.with_timezone(&Utc).to_rfc3339())
                }
                _ => Err(LogcatError::TimeConversion {
                    input: ts_threadtime.to_string(),
                    reason: format!("DST gap in timezone {}", anchor.tz),
                })
            }
        }
    }
}

/// Parse threadtime format to NaiveDateTime
fn parse_threadtime(ts: &str, anchor: &TimeAnchor) -> Result<NaiveDateTime> {
    let (md, rest) = ts.split_once(' ')
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "missing space separator".to_string(),
        })?;

    let (mon_s, day_s) = md.split_once('-')
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "invalid month-day format".to_string(),
        })?;

    let (hms, ms_s) = rest.split_once('.')
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "missing milliseconds".to_string(),
        })?;

    let mut it = hms.split(':');
    let h: u32 = it.next()
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "missing hour".to_string(),
        })?
        .parse()
        .map_err(|_| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "invalid hour".to_string(),
        })?;

    let m: u32 = it.next()
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "missing minute".to_string(),
        })?
        .parse()
        .map_err(|_| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "invalid minute".to_string(),
        })?;

    let s: u32 = it.next()
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "missing second".to_string(),
        })?
        .parse()
        .map_err(|_| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "invalid second".to_string(),
        })?;

    let mon: u32 = mon_s.parse().map_err(|_| LogcatError::TimeConversion {
        input: ts.to_string(),
        reason: "invalid month".to_string(),
    })?;

    let day: u32 = day_s.parse().map_err(|_| LogcatError::TimeConversion {
        input: ts.to_string(),
        reason: "invalid day".to_string(),
    })?;

    let ms: u32 = ms_s
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .map_err(|_| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: "invalid milliseconds".to_string(),
        })?;

    // Infer year based on reference date
    let reference = anchor.report_date
        .unwrap_or_else(|| Local::now().date_naive());
    let year = infer_year(mon, day, reference);

    let date = NaiveDate::from_ymd_opt(year, mon, day)
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: format!("invalid date: {}-{}-{}", year, mon, day),
        })?;

    let time = NaiveTime::from_hms_milli_opt(h, m, s, ms)
        .ok_or_else(|| LogcatError::TimeConversion {
            input: ts.to_string(),
            reason: format!("invalid time: {}:{}:{}.{}", h, m, s, ms),
        })?;

    Ok(NaiveDateTime::new(date, time))
}

/// Convert threadtime format to sortable numeric key
/// Used for time-based filtering without full ISO conversion
pub fn threadtime_ts_key(s: &str) -> anyhow::Result<u64> {
    let part = s.trim();
    let (md, rest) = part
        .split_once(' ')
        .ok_or_else(|| anyhow!("invalid threadtime ts"))?;
    let (mon_s, day_s) = md
        .split_once('-')
        .ok_or_else(|| anyhow!("invalid month-day"))?;
    let (hms, ms_s) = rest
        .split_once('.')
        .ok_or_else(|| anyhow!("invalid time.millis"))?;

    let mut it = hms.split(':');
    let h: u64 = it.next().ok_or_else(|| anyhow!("h"))?.parse()?;
    let m: u64 = it.next().ok_or_else(|| anyhow!("m"))?.parse()?;
    let s2: u64 = it.next().ok_or_else(|| anyhow!("s"))?.parse()?;
    let mon: u64 = mon_s.parse()?;
    let day: u64 = day_s.parse()?;
    let ms: u64 = ms_s
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()?;

    // Months normalized to 31 days window (approx for ordering)
    let key = (((mon * 32 + day) * 24 + h) * 60 + m) * 60 * 1000 + s2 * 1000 + ms;
    Ok(key)
}

/// Convert ISO 8601 timestamp to milliseconds since epoch
/// Note: For datetime without timezone info, we treat it as UTC to match
/// how device timestamps are stored in the database (device local time stored as UTC)
pub fn iso_ts_key_ms(s: &str) -> anyhow::Result<u64> {
    // Accept full RFC3339 (with timezone)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp_millis() as u64);
    }

    // Try common datetime-local shapes from <input type="datetime-local">
    let candidates = [
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.3f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.3f",
    ];

    for pat in candidates.iter() {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, pat) {
            // Treat as UTC to match database storage (device time stored as UTC)
            return Ok(naive.and_utc().timestamp_millis() as u64);
        }
    }

    Err(anyhow!("invalid datetime format"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::{Tz, Asia::Taipei};

    fn make_anchor(tz: Tz, year: i32) -> TimeAnchor {
        TimeAnchor {
            tz,
            year,
            report_date: NaiveDate::from_ymd_opt(year, 6, 15),
        }
    }

    #[test]
    fn test_to_iso_safe_normal() {
        let anchor = make_anchor(Taipei, 2024);
        let result = to_iso_safe("08-24 14:22:33.123", &anchor).unwrap();
        assert!(result.contains("2024-08-24"));
    }

    #[test]
    fn test_threadtime_ts_key() {
        let key1 = threadtime_ts_key("08-24 14:22:33.123").unwrap();
        let key2 = threadtime_ts_key("08-24 14:22:33.124").unwrap();
        assert!(key2 > key1);
    }

    #[test]
    fn test_threadtime_ts_key_ordering() {
        let key1 = threadtime_ts_key("08-24 14:22:33.000").unwrap();
        let key2 = threadtime_ts_key("08-24 14:22:34.000").unwrap();
        let key3 = threadtime_ts_key("08-24 14:23:00.000").unwrap();
        assert!(key1 < key2);
        assert!(key2 < key3);
    }

    #[test]
    fn test_iso_ts_key_ms_rfc3339() {
        let key = iso_ts_key_ms("2024-08-24T06:22:33.123+00:00").unwrap();
        assert!(key > 0);
    }

    #[test]
    fn test_iso_ts_key_ms_datetime_local() {
        let key = iso_ts_key_ms("2024-08-24T14:22").unwrap();
        assert!(key > 0);
    }
}
