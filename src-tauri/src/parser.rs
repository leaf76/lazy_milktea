use crate::types::{DeviceInfo, LogFilters, LogRow};
use regex::{Regex, RegexBuilder};
use anyhow::{anyhow, Context, Result};
use dirs::home_dir;
use chrono::{DateTime, Local, Utc, Datelike, NaiveDate, NaiveTime, NaiveDateTime};
use chrono::TimeZone;
use chrono_tz::Tz;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write, Seek, SeekFrom};
use std::collections::{VecDeque, HashMap};
use std::path::{Path, PathBuf};
use zip::read::ZipArchive;

pub struct ParseResult {
    pub device: DeviceInfo,
    pub anr_count: usize,
    pub crash_count: usize,
}

pub fn parse_entrypoint(path: &str) -> Result<ParseResult> {
    if is_zip(path) {
        parse_zip(path)
    } else {
        parse_txt(path)
    }
}

fn is_zip(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".zip")
}

fn parse_zip(path: &str) -> Result<ParseResult> {
    let file = File::open(path).with_context(|| format!("open zip: {}", path))?;
    let mut archive = ZipArchive::new(file).context("read zip archive")?;

    // Heuristic: choose the largest .txt whose name hints bugreport or main_entry
    let mut chosen_index: Option<usize> = None;
    let mut chosen_size: u64 = 0;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name().to_string();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".txt")
            && (lower.contains("bugreport") || lower.contains("main_entry") || lower.contains(".txt"))
        {
            let size = file.size();
            if size > chosen_size {
                chosen_index = Some(i);
                chosen_size = size;
            }
        }
    }

    let idx = chosen_index.ok_or_else(|| anyhow!("no main bugreport .txt found in zip"))?;
    let mut file = archive.by_index(idx).context("open chosen txt entry")?;

    // Stream read main entry
    let mut buf = String::new();
    file.read_to_string(&mut buf).context("read txt in zip")?;
    // Build logcat index into cache dir in background-ish (here synchronous)
    if let Ok(dir) = prepare_cache_dir(path) {
        let _ = index_logcat_from_zip(path, &dir).or_else(|_| index_logcat_from_text(&buf, &dir));
    }
    parse_from_reader(buf.as_bytes())
}

fn parse_txt(path: &str) -> Result<ParseResult> {
    let f = File::open(path).with_context(|| format!("open txt: {}", path))?;
    let reader = BufReader::new(f);
    parse_from_bufreader(reader)
}

fn parse_from_bufreader<R: BufRead>(reader: R) -> Result<ParseResult> {
    // Collect small subset for simple heuristics without loading entire file
    let mut device = DeviceInfo {
        brand: String::new(),
        model: String::new(),
        android_version: String::new(),
        api_level: 0,
        build_id: String::new(),
        fingerprint: String::new(),
        uptime_ms: 0,
        report_time: DateTime::<Utc>::from(Local::now()).to_rfc3339(),
        battery: None,
    };

    let re_fp = Regex::new(r"(?i)^\s*Build fingerprint:\s*(?P<fp>.+?)\s*$").unwrap();
    let re_sdk = Regex::new(r"(?i)\bro\.build\.version\.sdk\s*=\s*(?P<sdk>\d+)\b").unwrap();
    let re_rel = Regex::new(r"(?i)\bro\.build\.version\.release\s*=\s*(?P<rel>[^\s]+)\b").unwrap();
    let re_model = Regex::new(r"(?i)\bro\.product\.model\s*=\s*(?P<model>.+?)\s*$").unwrap();
    let re_brand = Regex::new(r"(?i)\bro\.product\.brand\s*=\s*(?P<brand>.+?)\s*$").unwrap();
    let re_build_id = Regex::new(r"(?i)\bro\.build\.id\s*=\s*(?P<bid>[^\s]+)\b").unwrap();
    let re_anr = Regex::new(r"(?i)\bANR in\b").unwrap();
    let re_fatal = Regex::new(r"(?i)FATAL EXCEPTION").unwrap();
    let re_tomb = Regex::new(r"(?i)\btombstone\b").unwrap();

    let mut anr_count = 0usize;
    let mut crash_count = 0usize;

    for line in reader.lines() {
        let line = line.unwrap_or_default();
        if device.fingerprint.is_empty() {
            if let Some(c) = re_fp.captures(&line) {
                device.fingerprint = c["fp"].trim().trim_matches('\'').to_string();
            }
        }
        if device.android_version.is_empty() {
            if let Some(c) = re_rel.captures(&line) {
                device.android_version = c["rel"].trim().to_string();
            }
        }
        if device.api_level == 0 {
            if let Some(c) = re_sdk.captures(&line) {
                device.api_level = c["sdk"].parse().unwrap_or(0);
            }
        }
        if device.model.is_empty() {
            if let Some(c) = re_model.captures(&line) {
                device.model = c["model"].trim().to_string();
            }
        }
        if device.brand.is_empty() {
            if let Some(c) = re_brand.captures(&line) {
                device.brand = c["brand"].trim().to_string();
            }
        }
        if device.build_id.is_empty() {
            if let Some(c) = re_build_id.captures(&line) {
                device.build_id = c["bid"].trim().to_string();
            }
        }

        if re_anr.is_match(&line) {
            anr_count += 1;
        }
        if re_fatal.is_match(&line) || re_tomb.is_match(&line) {
            crash_count += 1;
        }
    }

    Ok(ParseResult {
        device,
        anr_count,
        crash_count,
    })
}

fn parse_from_reader(mut reader: &[u8]) -> Result<ParseResult> {
    let buf = BufReader::new(&mut reader);
    parse_from_bufreader(buf)
}

// ---------- Cache & Logcat indexing ----------

pub fn prepare_cache_dir(report_path: &str) -> Result<PathBuf> {
    let home = home_dir().ok_or_else(|| anyhow!("cannot find home dir"))?;
    let name = Path::new(report_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("report");
    let dir = home.join(".lazy_milktea_cache").join(name);
    std::fs::create_dir_all(&dir).context("create cache dir")?;
    Ok(dir)
}

/// Build logcat jsonl from either logcat file inside zip or from the main txt content.
/// If `main_text` is provided, it will be used when no dedicated logcat file is found.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LogIndexSummary {
    pub total_rows: usize,
    pub ef_total: usize,
    pub recent_ef: usize,
    pub recent_window: usize,
    pub iso_min_ms: Option<u64>,
    pub iso_max_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct TimeAnchor { tz: Tz, year: i32 }

fn derive_time_anchor(text: &str) -> TimeAnchor {
    let tz_re = Regex::new(r"(?m)^\s*persist\.sys\.timezone\s*=\s*(?P<tz>\S+)\s*$").unwrap();
    let mut tz: Tz = chrono_tz::UTC;
    for cap in tz_re.captures_iter(text) {
        if let Ok(t) = cap["tz"].parse::<Tz>() { tz = t; break; }
    }
    let year = Local::now().year();
    TimeAnchor { tz, year }
}

fn to_iso(ts_threadtime: &str, anchor: &TimeAnchor) -> Option<String> {
    // ts_threadtime: MM-DD HH:MM:SS.mmm
    let (md, rest) = ts_threadtime.split_once(' ')?;
    let (mon_s, day_s) = md.split_once('-')?;
    let (hms, ms_s) = rest.split_once('.')?;
    let mut it = hms.split(':');
    let (h, m, s) = (
        it.next()?.parse::<u32>().ok()?,
        it.next()?.parse::<u32>().ok()?,
        it.next()?.parse::<u32>().ok()?,
    );
    let mon = mon_s.parse::<u32>().ok()?;
    let day = day_s.parse::<u32>().ok()?;

    // Cross-year heuristic: adjust around anchor year if far from today
    let today = Local::now().date_naive();
    let mut year = anchor.year;
    if let Some(d0) = NaiveDate::from_ymd_opt(year, mon, day) {
        let diff = d0.signed_duration_since(today).num_days().abs();
        if diff > 183 {
            // far from today; try shift ±1 year to be closer
            let alt_year = if d0 < today { year + 1 } else { year - 1 };
            if NaiveDate::from_ymd_opt(alt_year, mon, day).is_some() { year = alt_year; }
        }
    }

    let date = NaiveDate::from_ymd_opt(year, mon, day)?;
    let time = NaiveTime::from_hms_milli_opt(h, m, s, ms_s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok()?)?;
    let naive = NaiveDateTime::new(date, time);
    let local_dt = anchor.tz.from_local_datetime(&naive).single()?;
    let utc = local_dt.with_timezone(&Utc);
    Some(utc.to_rfc3339())
}

pub fn build_logcat_index(report_path: &str, cache_dir: &Path, main_text: Option<&str>) -> Result<LogIndexSummary> {
    if is_zip(report_path) {
        if let Ok(s) = index_logcat_from_zip(report_path, cache_dir) {
            if s.total_rows > 0 {
                return Ok(s);
            }
        }
        if let Some(text) = main_text {
            return index_logcat_from_text(text, cache_dir);
        }
        // fallback: try open main txt again
        let file = File::open(report_path).with_context(|| format!("open zip: {}", report_path))?;
        let mut archive = ZipArchive::new(file).context("read zip archive")?;
        let mut chosen_index: Option<usize> = None;
        let mut chosen_size: u64 = 0;
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name().to_string();
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".txt") && lower.contains("bugreport") {
                let size = file.size();
                if size > chosen_size {
                    chosen_index = Some(i);
                    chosen_size = size;
                }
            }
        }
        if let Some(idx) = chosen_index {
            let mut f = archive.by_index(idx).context("open chosen txt entry")?;
            let mut buf = String::new();
            f.read_to_string(&mut buf).context("read txt in zip")?;
            return index_logcat_from_text(&buf, cache_dir);
        }
        Ok(LogIndexSummary::default())
    } else {
        // plain txt
        let mut s = String::new();
        File::open(report_path)
            .and_then(|mut f| f.read_to_string(&mut s))
            .context("read plain txt for logcat indexing")?;
        index_logcat_from_text(&s, cache_dir)
    }
}

fn index_logcat_from_zip(path: &str, cache_dir: &Path) -> Result<LogIndexSummary> {
    let file = File::open(path).with_context(|| format!("open zip: {}", path))?;
    let mut archive = ZipArchive::new(file).context("read zip archive")?;
    let mut idx_file: Option<usize> = None;
    let mut best_size = 0u64;
    for i in 0..archive.len() {
        let f = archive.by_index(i)?;
        let name = f.name().to_ascii_lowercase();
        if name.contains("logcat") && !name.contains("events") && !name.ends_with("/") {
            let size = f.size();
            if size > best_size {
                best_size = size;
                idx_file = Some(i);
            }
        }
    }
    if let Some(i) = idx_file {
        let mut f = archive.by_index(i).context("open logcat entry")?;
        let mut buf = String::new();
        f.read_to_string(&mut buf).context("read logcat entry")?;
        return index_logcat_from_text(&buf, cache_dir);
    }
    Ok(LogIndexSummary::default())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct TimeIndexEntry { ts_key_minute: u64, offset: u64 }

fn index_logcat_from_text(text: &str, cache_dir: &Path) -> Result<LogIndexSummary> {
    let re = Regex::new(
        r"(?m)^(?P<date>\d{2}-\d{2})\s+(?P<time>\d{2}:\d{2}:\d{2}\.\d{3})\s+(?P<pid>\d+)\s+(?P<tid>\d+)\s+(?P<level>[VDIWEF])\s+(?P<tag>[^:]+):\s(?P<msg>.*)$",
    )
    .unwrap();

    let out_path = cache_dir.join("logcat.jsonl");
    let mut out = std::fs::File::create(&out_path).context("create logcat.jsonl")?;
    let mut count = 0usize;
    let mut ef_total = 0usize;
    let window = 2000usize;
    let mut last_levels: VecDeque<char> = VecDeque::with_capacity(window);
    let mut iso_min_ms: Option<u64> = None;
    let mut iso_max_ms: Option<u64> = None;
    let anchor = derive_time_anchor(text);
    // time index & offset tracking
    let mut byte_offset: u64 = 0;
    let mut time_index: Vec<TimeIndexEntry> = Vec::new();
    let mut last_minute_key: Option<u64> = None;
    // inverted index (sampling)
    const SAMPLE: usize = 3;
    let mut tag_count: HashMap<String, usize> = HashMap::new();
    let mut pid_count: HashMap<i32, usize> = HashMap::new();
    let mut inv_tag: HashMap<String, Vec<u64>> = HashMap::new();
    let mut inv_pid: HashMap<i32, Vec<u64>> = HashMap::new();

    for caps in re.captures_iter(text) {
        let ts = format!("{} {}", &caps["date"], &caps["time"]);
        let ts_iso = to_iso(&ts, &anchor);
        let row = LogRow {
            ts,
            ts_iso,
            level: caps["level"].to_string(),
            tag: caps["tag"].to_string(),
            pid: caps["pid"].parse().unwrap_or_default(),
            tid: caps["tid"].parse().unwrap_or_default(),
            msg: caps["msg"].to_string(),
        };
        let line = serde_json::to_string(&row)?;
        out.write_all(line.as_bytes())?;
        out.write_all(b"\n")?;
        count += 1;
        let lvl = row.level.chars().next().unwrap_or('I');
        if lvl == 'E' || lvl == 'F' { ef_total += 1; }
        if last_levels.len() == window { last_levels.pop_front(); }
        last_levels.push_back(lvl);
        // time index per minute (first log per minute records byte offset)
        if let Some(ref iso) = row.ts_iso {
            if let Ok(ts_key) = iso_ts_key_ms(iso) {
                // min/max epoch
                iso_min_ms = Some(iso_min_ms.map_or(ts_key, |m| m.min(ts_key)));
                iso_max_ms = Some(iso_max_ms.map_or(ts_key, |m| m.max(ts_key)));
                let minute_key = ts_key / 60_000;
                if last_minute_key != Some(minute_key) {
                    time_index.push(TimeIndexEntry { ts_key_minute: minute_key, offset: byte_offset });
                    last_minute_key = Some(minute_key);
                }
            }
        } else if let Ok(ts_key) = threadtime_ts_key(&row.ts) {
            let minute_key = ts_key / 60_000;
            if last_minute_key != Some(minute_key) {
                time_index.push(TimeIndexEntry { ts_key_minute: minute_key, offset: byte_offset });
                last_minute_key = Some(minute_key);
            }
        }
        // inverted index sampling
        {
            let c = tag_count.entry(row.tag.clone()).or_insert(0);
            if *c % SAMPLE == 0 { inv_tag.entry(row.tag.clone()).or_default().push(byte_offset); }
            *c += 1;
        }
        {
            let c = pid_count.entry(row.pid).or_insert(0);
            if *c % SAMPLE == 0 { inv_pid.entry(row.pid).or_default().push(byte_offset); }
            *c += 1;
        }

        // advance byte offset for next line
        byte_offset += (line.as_bytes().len() as u64) + 1u64;
    }
    let recent_ef = last_levels.iter().filter(|&&c| c == 'E' || c == 'F').count();
    let summary = LogIndexSummary { total_rows: count, ef_total, recent_ef, recent_window: window, iso_min_ms, iso_max_ms };
    // persist summary
    let sum_path = cache_dir.join("log_summary.json");
    let _ = std::fs::write(&sum_path, serde_json::to_vec(&summary)?);
    // persist time index
    let tix_path = cache_dir.join("log_time_index.json");
    let _ = std::fs::write(&tix_path, serde_json::to_vec(&time_index)?);
    // persist inverted index (tag/pid)
    let _ = std::fs::write(cache_dir.join("log_inv_tag.json"), serde_json::to_vec(&inv_tag)?);
    let _ = std::fs::write(cache_dir.join("log_inv_pid.json"), serde_json::to_vec(&inv_pid)?);
    Ok(summary)
}

pub fn query_logcat(cache_dir: &Path, filters: &LogFilters, page: usize, page_size: usize) -> Result<Vec<LogRow>> {
    let path = cache_dir.join("logcat.jsonl");
    let f = File::open(&path).with_context(|| format!("open logcat index: {}", path.display()))?;
    let reader = BufReader::new(f);

    let mut out = Vec::with_capacity(page_size);
    let start = page.saturating_mul(page_size);
    let end = start + page_size;
    let mut matched = 0usize;

    let levels_set: Option<std::collections::HashSet<String>> = filters
        .levels
        .as_ref()
        .map(|v| v.iter().map(|s| s.to_string()).collect());
    let tag_q = filters.tag.as_deref();
    let text_q = filters.text.as_deref();
    let not_text_q = filters.not_text.as_deref();
    let pid_q = filters.pid;
    let tid_q = filters.tid;
    let case_sensitive = filters.case_sensitive.unwrap_or(false);
    let text_mode = filters.text_mode.as_deref().unwrap_or("plain");
    let text_re: Option<Regex> = if text_mode == "regex" {
        text_q.and_then(|p| RegexBuilder::new(p).case_insensitive(!case_sensitive).build().ok())
    } else { None };
    let not_text_re: Option<Regex> = if text_mode == "regex" {
        not_text_q.and_then(|p| RegexBuilder::new(p).case_insensitive(!case_sensitive).build().ok())
    } else { None };
    let ts_from_key = filters
        .ts_from
        .as_deref()
        .and_then(|s| threadtime_ts_key(s).ok());
    let ts_to_key = filters
        .ts_to
        .as_deref()
        .and_then(|s| threadtime_ts_key(s).ok());

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        let mut ok = true;
        if let Ok(row) = serde_json::from_str::<LogRow>(&line) {
            // time range
            if ts_from_key.is_some() || ts_to_key.is_some() {
                if let Ok(k) = threadtime_ts_key(&row.ts) {
                    if let Some(fk) = ts_from_key { ok &= k >= fk; }
                    if let Some(tk) = ts_to_key { ok &= k <= tk; }
                }
            }
            if let Some(levels) = &levels_set {
                ok &= levels.contains(&row.level);
            }
            if let Some(tag) = tag_q {
                ok &= row.tag.contains(tag);
            }
            if let Some(pid) = pid_q {
                ok &= row.pid == pid;
            }
            if let Some(tid) = tid_q {
                ok &= row.tid == tid;
            }
            if let Some(text) = text_q {
                if text_mode == "regex" {
                    if let Some(re) = &text_re { ok &= re.is_match(&row.msg); }
                } else if case_sensitive {
                    ok &= row.msg.contains(text);
                } else {
                    ok &= row.msg.to_lowercase().contains(&text.to_lowercase());
                }
            }
            if let Some(neg) = not_text_q {
                if text_mode == "regex" {
                    if let Some(re) = &not_text_re { ok &= !re.is_match(&row.msg); }
                } else if case_sensitive {
                    ok &= !row.msg.contains(neg);
                } else {
                    ok &= !row.msg.to_lowercase().contains(&neg.to_lowercase());
                }
            }

            if ok {
                if matched >= start && matched < end {
                    out.push(row);
                }
                matched += 1;
                if matched >= end {
                    break;
                }
            }
        }
    }
    Ok(out)
}

fn threadtime_ts_key(s: &str) -> Result<u64> {
    // s example: "08-24 14:22:33.123" or "08-24 14:22:33.123Z" (we only parse first part)
    // Convert to sortable numeric key across month/day/hour/min/sec/ms
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
    // months normalized to 31 days window (approx for ordering)
    let key = (((mon * 32 + day) * 24 + h) * 60 + m) * 60 * 1000 + s2 * 1000 + ms;
    Ok(key)
}

fn iso_ts_key_ms(s: &str) -> Result<u64> {
    // Accept full RFC3339 (with timezone) or datetime-local (no timezone)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp_millis() as u64);
    }
    // Try common datetime-local shapes from <input type="datetime-local">, assume local timezone
    // Patterns without seconds and with seconds, optional milliseconds
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
            let local_dt = Local.from_local_datetime(&naive).single()
                .ok_or_else(|| anyhow!("ambiguous local time"))?;
            return Ok(local_dt.with_timezone(&Utc).timestamp_millis() as u64);
        }
    }
    Err(anyhow!("invalid datetime format"))
}

fn load_time_index(cache_dir: &Path) -> Result<Vec<TimeIndexEntry>> {
    let path = cache_dir.join("log_time_index.json");
    let data = std::fs::read(&path).with_context(|| format!("open time index: {}", path.display()))?;
    let v: Vec<TimeIndexEntry> = serde_json::from_slice(&data)?;
    Ok(v)
}

fn seek_offset_for_ts(cache_dir: &Path, ts_from: &str) -> Option<u64> {
    let idx = load_time_index(cache_dir).ok()?;
    let key = if ts_from.contains('T') {
        iso_ts_key_ms(ts_from).ok()? / 60_000
    } else {
        threadtime_ts_key(ts_from).ok()? / 60_000
    };
    if idx.is_empty() { return Some(0); }
    // binary search first entry >= key
    let mut lo = 0usize; let mut hi = idx.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if idx[mid].ts_key_minute < key { lo = mid + 1; } else { hi = mid; }
    }
    if lo >= idx.len() { Some(idx.last().map(|e| e.offset).unwrap_or(0)) } else { Some(idx[lo].offset) }
}

pub struct StreamResult {
    pub rows: Vec<LogRow>,
    pub next_cursor: u64,
    pub exhausted: bool,
    pub file_size: u64,
    pub total_rows: Option<usize>,
}

pub fn stream_logcat(cache_dir: &Path, filters: &LogFilters, cursor: Option<u64>, limit: usize) -> Result<StreamResult> {
    let path = cache_dir.join("logcat.jsonl");
    let mut f = File::open(&path).with_context(|| format!("open logcat index: {}", path.display()))?;
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let total_rows = std::fs::read(cache_dir.join("log_summary.json")).ok()
        .and_then(|b| serde_json::from_slice::<LogIndexSummary>(&b).ok())
        .map(|s| s.total_rows);
    let base_offset = if let Some(c) = cursor { c } else if let Some(ref tsf) = filters.ts_from { seek_offset_for_ts(cache_dir, tsf).unwrap_or(0) } else { 0 };

    // hint from inverted index if tag/pid present
    let mut hint_offsets: Option<Vec<u64>> = None;
    if let Some(ref tag) = filters.tag {
        if let Ok(v) = load_inv_tag(cache_dir) { if let Some(list) = v.get(tag) { hint_offsets = Some(list.clone()); } }
    }
    if let Some(pid) = filters.pid {
        if let Ok(v) = load_inv_pid(cache_dir) {
            if let Some(list) = v.get(&pid) {
                hint_offsets = Some(match hint_offsets {
                    Some(a) => {
                        // intersect two sorted lists (they are in order as we scanned forward)
                        let b = list; let mut i=0usize; let mut j=0usize; let mut out=Vec::new();
                        while i < a.len() && j < b.len() { if a[i] == b[j] { out.push(a[i]); i+=1; j+=1; } else if a[i] < b[j] { i+=1; } else { j+=1; } }
                        if out.is_empty() { a } else { out }
                    }
                    None => list.clone()
                });
            }
        }
    }

    let start_offset = if let Some(hints) = &hint_offsets {
        // find first hint >= base_offset
        let mut lo = 0usize; let mut hi = hints.len();
        while lo < hi { let mid=(lo+hi)/2; if hints[mid] < base_offset { lo = mid+1; } else { hi = mid; } }
        hints.get(lo).copied().unwrap_or(base_offset)
    } else { base_offset };
    f.seek(SeekFrom::Start(start_offset)).ok();
    let mut reader = BufReader::new(f);

    let mut out: Vec<LogRow> = Vec::with_capacity(limit);
    let mut next_cursor = start_offset;

    let levels_set: Option<std::collections::HashSet<String>> = filters
        .levels
        .as_ref()
        .map(|v| v.iter().map(|s| s.to_string()).collect());
    let tag_q = filters.tag.as_deref();
    let text_q = filters.text.as_deref();
    let not_text_q = filters.not_text.as_deref();
    let pid_q = filters.pid;
    let tid_q = filters.tid;
    let case_sensitive = filters.case_sensitive.unwrap_or(false);
    let text_mode = filters.text_mode.as_deref().unwrap_or("plain");
    let text_re: Option<Regex> = if text_mode == "regex" {
        text_q.and_then(|p| RegexBuilder::new(p).case_insensitive(!case_sensitive).build().ok())
    } else { None };
    let not_text_re: Option<Regex> = if text_mode == "regex" {
        not_text_q.and_then(|p| RegexBuilder::new(p).case_insensitive(!case_sensitive).build().ok())
    } else { None };
    let ts_to_key = filters.ts_to.as_deref().and_then(|s| {
        if s.contains('T') { iso_ts_key_ms(s).ok() } else { threadtime_ts_key(s).ok() }
    });

    let mut buf = String::new();
    loop {
        buf.clear();
        let read = reader.read_line(&mut buf)?; // includes trailing \n
        if read == 0 { break; }
        next_cursor += read as u64;
        let line = buf.trim_end_matches(['\n', '\r']);
        if line.is_empty() { continue; }
        if let Ok(row) = serde_json::from_str::<LogRow>(line) {
            if let Some(tk) = ts_to_key {
                if let Ok(k) = threadtime_ts_key(&row.ts) { if k > tk { break; } }
            }
            let mut ok = true;
            if let Some(levels) = &levels_set { ok &= levels.contains(&row.level); }
            if let Some(tag) = tag_q { ok &= row.tag.contains(tag); }
            if let Some(pid) = pid_q { ok &= row.pid == pid; }
            if let Some(tid) = tid_q { ok &= row.tid == tid; }
            if let Some(text) = text_q {
                if text_mode == "regex" {
                    if let Some(re) = &text_re { ok &= re.is_match(&row.msg); }
                } else if case_sensitive {
                    ok &= row.msg.contains(text);
                } else {
                    ok &= row.msg.to_lowercase().contains(&text.to_lowercase());
                }
            }
            if let Some(neg) = not_text_q {
                if text_mode == "regex" {
                    if let Some(re) = &not_text_re { ok &= !re.is_match(&row.msg); }
                } else if case_sensitive {
                    ok &= !row.msg.contains(neg);
                } else {
                    ok &= !row.msg.to_lowercase().contains(&neg.to_lowercase());
                }
            }
            if ok {
                out.push(row);
                if out.len() >= limit { break; }
            }
        }
    }
    // mark exhausted if at EOF
    let exhausted = out.is_empty() && reader.fill_buf()?.is_empty();
    Ok(StreamResult { rows: out, next_cursor, exhausted, file_size, total_rows })
}

fn load_inv_tag(cache_dir: &Path) -> Result<HashMap<String, Vec<u64>>> {
    let path = cache_dir.join("log_inv_tag.json");
    let data = std::fs::read(&path).with_context(|| format!("open inv tag: {}", path.display()))?;
    Ok(serde_json::from_slice(&data)?)
}
fn load_inv_pid(cache_dir: &Path) -> Result<HashMap<i32, Vec<u64>>> {
    let path = cache_dir.join("log_inv_pid.json");
    let data = std::fs::read(&path).with_context(|| format!("open inv pid: {}", path.display()))?;
    Ok(serde_json::from_slice(&data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_device_info_from_snippet() {
        let sample = r#"
Build fingerprint: 'google/sunfish/sunfish:13/TQ3A.230605.012/abcd:user/release-keys'
ro.build.version.release=13
ro.build.version.sdk=33
ro.product.brand=google
ro.product.model=Pixel 4a
ro.build.id=TQ3A.230605.012
--------- beginning of crash
FATAL EXCEPTION: main
ANR in com.example.app (pid 1234)
"#;
        let r = parse_from_reader(sample.as_bytes()).unwrap();
        assert_eq!(r.device.brand, "google");
        assert_eq!(r.device.model, "Pixel 4a");
        assert_eq!(r.device.android_version, "13");
        assert_eq!(r.device.api_level, 33);
        assert_eq!(r.device.build_id, "TQ3A.230605.012");
        assert!(r.device.fingerprint.contains("sunfish"));
        assert!(r.crash_count >= 1);
        assert!(r.anr_count >= 1);
    }

    #[test]
    fn index_and_query_logcat_from_text() {
        let sample = r#"
08-24 14:22:33.123  1234  5678 E ActivityManager: ANR in com.foo
08-24 14:22:34.999  1234  5678 I MyTag: hello world
08-24 14:22:35.001  2222  5679 W Network: unstable
"#;
        // temp dir
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let tmp = std::env::temp_dir().join(format!("lm_idx_{}", nanos));
        std::fs::create_dir_all(&tmp).unwrap();

        let sum = index_logcat_from_text(sample, &tmp).unwrap();
        assert_eq!(sum.total_rows, 3);
        assert!(sum.ef_total >= 1);

        // query E level only
        let filters = LogFilters { levels: Some(vec!["E".into()]), ..Default::default() };
        let rows = query_logcat(&tmp, &filters, 0, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].level, "E");
        assert!(rows[0].tag.contains("ActivityManager"));
    }

    #[test]
    fn logcat_additional_filters_tid_and_exclude() {
        let sample = r#"
08-24 14:22:33.000  1000  2000 I TagA: hello apple
08-24 14:22:34.000  1000  2001 I TagA: hello banana
08-24 14:22:35.000  1001  2000 I TagB: HELLO CHERRY
"#;
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let tmp = std::env::temp_dir().join(format!("lm_idx2_{}", nanos));
        std::fs::create_dir_all(&tmp).unwrap();
        let _ = index_logcat_from_text(sample, &tmp).unwrap();

        // filter by tid
        let rows_tid = query_logcat(&tmp, &LogFilters { tid: Some(2001), ..Default::default() }, 0, 100).unwrap();
        assert_eq!(rows_tid.len(), 1);
        assert!(rows_tid[0].msg.contains("banana"));

        // include text (case-insensitive) and exclude another
        let rows_text = query_logcat(&tmp, &LogFilters { text: Some("hello".into()), not_text: Some("banana".into()), ..Default::default() }, 0, 100).unwrap();
        assert_eq!(rows_text.len(), 2); // apple, CHERRY

        // regex case-sensitive match should only catch HELLO
        let rows_regex = query_logcat(&tmp, &LogFilters { text: Some("HELLO".into()), text_mode: Some("regex".into()), case_sensitive: Some(true), ..Default::default() }, 0, 100).unwrap();
        assert_eq!(rows_regex.len(), 1);
        assert!(rows_regex[0].msg.contains("CHERRY"));
    }
}
