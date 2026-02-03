use crate::error::{LogcatError, Result};
use crate::query::cursor::{QueryCursor, QueryResponse, CursorDirection, LogcatStats, LevelCounts};
use crate::query::filter::{compile_user_regex, plain_text_contains, should_use_plain_search};
use crate::types::{LogFilters, LogRow};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

/// Query executor for SQLite-based logcat index
pub struct QueryExecutor {
    conn: Connection,
}

struct RowWithMeta {
    id: i64,
    ts_unix: f64,
    row: LogRow,
}

impl QueryExecutor {
    /// Open a query executor for existing database
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(Self { conn })
    }

    /// Execute cursor-based query
    pub fn query(
        &self,
        filters: &LogFilters,
        cursor: Option<&QueryCursor>,
        limit: usize,
        direction: CursorDirection,
    ) -> Result<QueryResponse> {
        let filter_hash = compute_filter_hash(filters);

        // Validate cursor if provided
        // Note: Hash validation disabled - frontend manages filter changes by resetting cursor
        if let Some(c) = cursor {
            if c.filter_hash != filter_hash && c.filter_hash != 0 {
                // Log mismatch but don't reject - frontend handles filter changes
                log::warn!(
                    "filter hash mismatch: cursor={}, computed={}",
                    c.filter_hash,
                    filter_hash
                );
            }
        }

        // Build and execute query
        let rows = self.execute_query(filters, cursor, limit, direction, false, None)?;
        Ok(self.build_response(rows, filter_hash, cursor, direction, limit))
    }

    /// Execute legacy offset-based query
    pub fn query_by_offset(
        &self,
        filters: &LogFilters,
        offset: i64,
        limit: usize,
    ) -> Result<Vec<LogRow>> {
        let rows = self.execute_query_with_offset(filters, offset, limit)?;
        Ok(rows.into_iter().map(|r| r.row).collect())
    }

    /// Execute jump-to-time query (inclusive)
    pub fn query_from_time(
        &self,
        filters: &LogFilters,
        target_ts_unix: f64,
        limit: usize,
    ) -> Result<QueryResponse> {
        let filter_hash = compute_filter_hash(filters);
        let rows = self.execute_query(
            filters,
            None,
            limit,
            CursorDirection::Forward,
            true,
            Some(target_ts_unix),
        )?;
        Ok(self.build_response(rows, filter_hash, None, CursorDirection::Forward, limit))
    }

    /// Build cursor from row id for legacy APIs
    pub fn cursor_from_id(&self, id: i64, filter_hash: u64) -> Result<Option<QueryCursor>> {
        let ts_unix: Option<f64> = self
            .conn
            .query_row(
                "SELECT ts_unix FROM logs WHERE id = ?",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        Ok(ts_unix.map(|ts| QueryCursor::new(id, ts, CursorDirection::Forward, filter_hash)))
    }

    fn build_response(
        &self,
        rows: Vec<RowWithMeta>,
        filter_hash: u64,
        cursor: Option<&QueryCursor>,
        direction: CursorDirection,
        limit: usize,
    ) -> QueryResponse {
        let has_more = rows.len() >= limit;
        let first = rows.first();
        let last = rows.last();

        let make_cursor = |row: &RowWithMeta, dir: CursorDirection| {
            QueryCursor::new(row.id, row.ts_unix, dir, filter_hash)
        };

        let (next_cursor, prev_cursor) = match direction {
            CursorDirection::Forward => {
                let next = if has_more { last.map(|r| make_cursor(r, CursorDirection::Forward)) } else { None };
                let prev = if cursor.is_some() { first.map(|r| make_cursor(r, CursorDirection::Backward)) } else { None };
                (next, prev)
            }
            CursorDirection::Backward => {
                let prev = if has_more { last.map(|r| make_cursor(r, CursorDirection::Backward)) } else { None };
                let next = if cursor.is_some() { first.map(|r| make_cursor(r, CursorDirection::Forward)) } else { None };
                (next, prev)
            }
        };

        let has_more_next = matches!(direction, CursorDirection::Forward) && has_more;
        let mut has_more_prev = matches!(direction, CursorDirection::Backward) && has_more;
        if matches!(direction, CursorDirection::Forward) && cursor.is_some() {
            has_more_prev = true;
        }

        QueryResponse {
            rows: rows.into_iter().map(|r| r.row).collect(),
            next_cursor,
            prev_cursor,
            has_more_next,
            has_more_prev,
            estimated_total: None,
            position_ratio: 0.0,
        }
    }

    fn execute_query(
        &self,
        filters: &LogFilters,
        cursor: Option<&QueryCursor>,
        limit: usize,
        direction: CursorDirection,
        include_cursor: bool,
        override_ts_from: Option<f64>,
    ) -> Result<Vec<RowWithMeta>> {
        let use_fts = should_use_fts(filters);
        let resolved_cursor = if let Some(c) = cursor {
            if c.ts_unix == 0.0 && c.position > 0 {
                self.cursor_from_id(c.position, c.filter_hash)?
            } else {
                None
            }
        } else {
            None
        };
        let cursor_ref = resolved_cursor.as_ref().or(cursor);

        let (where_clause, mut params) = self.build_where_clause(
            filters,
            cursor_ref,
            direction,
            include_cursor,
            use_fts,
            override_ts_from,
        )?;

        let order = match direction {
            CursorDirection::Forward => "ASC",
            CursorDirection::Backward => "DESC",
        };

        let sql = format!(
            "SELECT id, ts_unix, ts_display, ts_iso, level, tag, pid, tid, msg FROM logs {} ORDER BY ts_unix {}, id {} LIMIT ?",
            where_clause,
            order,
            order,
        );

        // Add limit as the last parameter
        params.push(Box::new(limit as i64));

        let mut stmt = self.conn.prepare(&sql)
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        // Convert params to references for rusqlite
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows: Vec<RowWithMeta> = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(RowWithMeta {
                    id: row.get(0)?,
                    ts_unix: row.get(1)?,
                    row: LogRow {
                        ts: row.get(2)?,
                        ts_iso: row.get(3)?,
                        level: row.get(4)?,
                        tag: row.get(5)?,
                        pid: row.get(6)?,
                        tid: row.get(7)?,
                        msg: row.get(8)?,
                    },
                })
            })
            .map_err(|e| LogcatError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        // Apply text filter in Rust if using regex or exclude filters
        let rows = self.apply_text_filters(rows, filters)?;

        Ok(rows)
    }

    fn execute_query_with_offset(
        &self,
        filters: &LogFilters,
        offset: i64,
        limit: usize,
    ) -> Result<Vec<RowWithMeta>> {
        let use_fts = should_use_fts(filters);
        let (where_clause, mut params) =
            self.build_where_clause(filters, None, CursorDirection::Forward, false, use_fts, None)?;

        let sql = format!(
            "SELECT id, ts_unix, ts_display, ts_iso, level, tag, pid, tid, msg FROM logs {} ORDER BY ts_unix ASC, id ASC LIMIT ? OFFSET ?",
            where_clause
        );

        params.push(Box::new(limit as i64));
        params.push(Box::new(offset));

        let mut stmt = self.conn.prepare(&sql)
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows: Vec<RowWithMeta> = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(RowWithMeta {
                    id: row.get(0)?,
                    ts_unix: row.get(1)?,
                    row: LogRow {
                        ts: row.get(2)?,
                        ts_iso: row.get(3)?,
                        level: row.get(4)?,
                        tag: row.get(5)?,
                        pid: row.get(6)?,
                        tid: row.get(7)?,
                        msg: row.get(8)?,
                    },
                })
            })
            .map_err(|e| LogcatError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        let rows = self.apply_text_filters(rows, filters)?;
        Ok(rows)
    }

    fn build_where_clause(
        &self,
        filters: &LogFilters,
        cursor: Option<&QueryCursor>,
        direction: CursorDirection,
        include_cursor: bool,
        use_fts: bool,
        override_ts_from: Option<f64>,
    ) -> Result<(String, Vec<Box<dyn rusqlite::ToSql>>)> {
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Level filter (parameterized)
        if let Some(ref levels) = filters.levels {
            if !levels.is_empty() {
                let placeholders: Vec<&str> = levels.iter().map(|_| "?").collect();
                conditions.push(format!("level IN ({})", placeholders.join(",")));
                for level in levels {
                    params.push(Box::new(level.clone()));
                }
            }
        }

        // Tag filter (parameterized substring match, supports OR with |)
        if let Some(ref tag) = filters.tag {
            let tags: Vec<&str> = tag.split('|').map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
            if tags.len() == 1 {
                conditions.push("tag LIKE ?".to_string());
                params.push(Box::new(format!("%{}%", tags[0])));
            } else if tags.len() > 1 {
                let placeholders: Vec<&str> = tags.iter().map(|_| "tag LIKE ?").collect();
                conditions.push(format!("({})", placeholders.join(" OR ")));
                for t in tags {
                    params.push(Box::new(format!("%{}%", t)));
                }
            }
        }

        // PID filter (parameterized)
        if let Some(pid) = filters.pid {
            conditions.push("pid = ?".to_string());
            params.push(Box::new(pid));
        }

        // TID filter (parameterized)
        if let Some(tid) = filters.tid {
            conditions.push("tid = ?".to_string());
            params.push(Box::new(tid));
        }

        // Time range filter (parameterized)
        if let Some(ms) = override_ts_from {
            conditions.push("ts_unix >= ?".to_string());
            params.push(Box::new(ms));
        } else if let Some(ref ts_from) = filters.ts_from {
            if let Ok(ms) = crate::time::iso_ts_key_ms(ts_from) {
                conditions.push("ts_unix >= ?".to_string());
                params.push(Box::new(ms as f64));
            }
        }

        if let Some(ref ts_to) = filters.ts_to {
            if let Ok(ms) = crate::time::iso_ts_key_ms(ts_to) {
                conditions.push("ts_unix <= ?".to_string());
                params.push(Box::new(ms as f64));
            }
        }

        let mode = filters.text_mode.as_deref().unwrap_or("plain");
        let case_sensitive = filters.case_sensitive.unwrap_or(false);

        // FTS for plain text search
        if use_fts {
            if let Some(ref text) = filters.text {
                let fts_query = escape_fts_query(text);
                conditions.push("id IN (SELECT rowid FROM logs_fts WHERE logs_fts MATCH ?)".to_string());
                params.push(Box::new(fts_query));
            }
        } else if mode == "plain" {
            if let Some(ref text) = filters.text {
                if !text.trim().is_empty() {
                    if case_sensitive {
                        conditions.push("instr(msg, ?) > 0".to_string());
                        params.push(Box::new(text.clone()));
                    } else {
                        conditions.push("instr(lower(msg), lower(?)) > 0".to_string());
                        params.push(Box::new(text.clone()));
                    }
                }
            }
        }

        if mode == "plain" {
            if let Some(ref not_text) = filters.not_text {
                if !not_text.trim().is_empty() {
                    if case_sensitive {
                        conditions.push("instr(msg, ?) = 0".to_string());
                        params.push(Box::new(not_text.clone()));
                    } else {
                        conditions.push("instr(lower(msg), lower(?)) = 0".to_string());
                        params.push(Box::new(not_text.clone()));
                    }
                }
            }
        }

        // Cursor position (parameterized)
        if let Some(c) = cursor {
            match direction {
                CursorDirection::Forward => {
                    let op = if include_cursor { ">=" } else { ">" };
                    conditions.push(format!("(ts_unix > ? OR (ts_unix = ? AND id {} ?))", op));
                    params.push(Box::new(c.ts_unix));
                    params.push(Box::new(c.ts_unix));
                    params.push(Box::new(c.position));
                }
                CursorDirection::Backward => {
                    let op = if include_cursor { "<=" } else { "<" };
                    conditions.push(format!("(ts_unix < ? OR (ts_unix = ? AND id {} ?))", op));
                    params.push(Box::new(c.ts_unix));
                    params.push(Box::new(c.ts_unix));
                    params.push(Box::new(c.position));
                }
            }
        }

        // Build SQL
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        Ok((where_clause, params))
    }

    fn apply_text_filters(&self, rows: Vec<RowWithMeta>, filters: &LogFilters) -> Result<Vec<RowWithMeta>> {
        let mode = filters.text_mode.as_deref().unwrap_or("plain");
        let case_sensitive = filters.case_sensitive.unwrap_or(false);

        // Apply include text filter
        let rows = if let Some(ref text) = filters.text {
            if mode == "regex" {
                let re = compile_user_regex(text, !case_sensitive)?;
                rows.into_iter()
                    .filter(|r| re.is_match(&r.row.msg))
                    .collect()
            } else {
                rows.into_iter()
                    .filter(|r| plain_text_contains(&r.row.msg, text, case_sensitive))
                    .collect()
            }
        } else {
            rows
        };

        // Apply exclude text filter
        let rows = if let Some(ref not_text) = filters.not_text {
            if mode == "regex" {
                let re = compile_user_regex(not_text, !case_sensitive)?;
                rows.into_iter()
                    .filter(|r| !re.is_match(&r.row.msg))
                    .collect()
            } else {
                rows.into_iter()
                    .filter(|r| !plain_text_contains(&r.row.msg, not_text, case_sensitive))
                    .collect()
            }
        } else {
            rows
        };

        Ok(rows)
    }

    /// Get statistics about the logcat data
    pub fn get_stats(&self, filters: &LogFilters) -> Result<LogcatStats> {
        let total_rows: usize = self.conn
            .query_row("SELECT COUNT(*) FROM logs", [], |r| r.get(0))
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        let (min_ts, max_ts): (Option<i64>, Option<i64>) = self.conn
            .query_row(
                "SELECT MIN(CAST(ts_unix AS INTEGER)), MAX(CAST(ts_unix AS INTEGER)) FROM logs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        // Get the display timestamps (device local time) for min and max
        let min_ts_display: Option<String> = self.conn
            .query_row(
                "SELECT ts_display FROM logs ORDER BY ts_unix ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();

        let max_ts_display: Option<String> = self.conn
            .query_row(
                "SELECT ts_display FROM logs ORDER BY ts_unix DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();

        let level_counts = self.get_level_counts()?;

        let filtered_rows = self.get_filtered_count(filters)?;

        Ok(LogcatStats {
            total_rows,
            filtered_rows,
            min_timestamp_ms: min_ts,
            max_timestamp_ms: max_ts,
            min_ts_display,
            max_ts_display,
            level_counts,
        })
    }

    fn get_level_counts(&self) -> Result<LevelCounts> {
        let mut counts = LevelCounts::default();

        let mut stmt = self.conn
            .prepare("SELECT level, COUNT(*) FROM logs GROUP BY level")
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let level: String = row.get(0)?;
                let count: usize = row.get(1)?;
                Ok((level, count))
            })
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        for row in rows.flatten() {
            match row.0.as_str() {
                "V" => counts.verbose = row.1,
                "D" => counts.debug = row.1,
                "I" => counts.info = row.1,
                "W" => counts.warning = row.1,
                "E" => counts.error = row.1,
                "F" => counts.fatal = row.1,
                _ => {}
            }
        }

        Ok(counts)
    }

    fn get_filtered_count(&self, filters: &LogFilters) -> Result<Option<usize>> {
        if filters_are_empty(filters) {
            return Ok(None);
        }

        let use_fts = should_use_fts(filters);
        let mode = filters.text_mode.as_deref().unwrap_or("plain");
        let has_text = filters.text.as_ref().map(|t| !t.trim().is_empty()).unwrap_or(false);
        let has_not_text = filters.not_text.as_ref().map(|t| !t.trim().is_empty()).unwrap_or(false);

        if mode == "regex" || has_not_text {
            return Ok(None);
        }

        if has_text && !use_fts {
            return Ok(None);
        }

        let (where_clause, params) =
            self.build_where_clause(filters, None, CursorDirection::Forward, false, use_fts, None)?;

        let sql = format!("SELECT COUNT(*) FROM logs {}", where_clause);
        let mut stmt = self.conn.prepare(&sql)
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let count: usize = stmt
            .query_row(param_refs.as_slice(), |r| r.get(0))
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        Ok(Some(count))
    }
}

/// Compute hash of filter conditions for cursor validation
/// Uses JSON serialization for deterministic hash values across invocations
fn compute_filter_hash(filters: &LogFilters) -> u64 {
    // Serialize to JSON for deterministic representation
    let json = serde_json::to_string(filters).unwrap_or_default();
    // Simple string hash with fixed algorithm
    let mut hash: u64 = 5381;
    for byte in json.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

fn filters_are_empty(filters: &LogFilters) -> bool {
    let has_text = filters.text.as_ref().map_or(false, |v| !v.trim().is_empty());
    let has_not_text = filters.not_text.as_ref().map_or(false, |v| !v.trim().is_empty());
    filters.ts_from.is_none()
        && filters.ts_to.is_none()
        && filters.levels.as_ref().map_or(true, |v| v.is_empty())
        && filters.tag.as_ref().map_or(true, |v| v.trim().is_empty())
        && filters.pid.is_none()
        && filters.tid.is_none()
        && !has_text
        && !has_not_text
        && (!has_text || filters.text_mode.is_none())
        && (!has_text || filters.case_sensitive.unwrap_or(false) == false)
}

fn should_use_fts(filters: &LogFilters) -> bool {
    let text = match filters.text.as_deref() {
        Some(t) => t.trim(),
        None => return false,
    };

    if text.is_empty() {
        return false;
    }

    if filters.text_mode.as_deref().unwrap_or("plain") != "plain" {
        return false;
    }

    if filters.case_sensitive.unwrap_or(false) {
        return false;
    }

    if !should_use_plain_search(text) {
        return false;
    }

    if text.chars().any(|c| c.is_whitespace()) {
        return false;
    }

    text.len() >= 2
}

fn escape_fts_query(text: &str) -> String {
    let escaped = text.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::sqlite::LogcatDatabase;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("test_query_executor_{}.db", nanos))
    }

    fn seed_db() -> (std::path::PathBuf, QueryExecutor) {
        let path = temp_db_path();
        let db = LogcatDatabase::create(&path).unwrap();

        let rows = vec![
            ("t2000-a", 2000.0, "A"),
            ("t1000", 1000.0, "B"),
            ("t2000-b", 2000.0, "C"),
            ("t3000", 3000.0, "D"),
        ];

        for (ts, ts_unix, msg) in rows {
            let row = LogRow {
                ts: ts.to_string(),
                ts_iso: None,
                level: "I".to_string(),
                tag: "Test".to_string(),
                pid: 1000,
                tid: 1000,
                msg: msg.to_string(),
            };
            db.insert(&row, ts_unix).unwrap();
        }

        let executor = QueryExecutor::open(&path).unwrap();
        (path, executor)
    }

    #[test]
    fn test_compute_filter_hash() {
        let f1 = LogFilters {
            levels: Some(vec!["E".to_string()]),
            ..Default::default()
        };
        let f2 = LogFilters {
            levels: Some(vec!["E".to_string()]),
            ..Default::default()
        };
        let f3 = LogFilters {
            levels: Some(vec!["W".to_string()]),
            ..Default::default()
        };

        assert_eq!(compute_filter_hash(&f1), compute_filter_hash(&f2));
        assert_ne!(compute_filter_hash(&f1), compute_filter_hash(&f3));
    }

    #[test]
    fn test_cursor_ordering_by_ts_then_id() {
        let (path, executor) = seed_db();
        let filters = LogFilters::default();

        let resp = executor
            .query(&filters, None, 2, CursorDirection::Forward)
            .unwrap();

        assert_eq!(resp.rows.len(), 2);
        assert_eq!(resp.rows[0].ts, "t1000");
        assert_eq!(resp.rows[1].ts, "t2000-a");

        let next_cursor = resp.next_cursor.expect("missing next cursor");
        let resp2 = executor
            .query(&filters, Some(&next_cursor), 10, CursorDirection::Forward)
            .unwrap();

        let ts_list: Vec<String> = resp2.rows.iter().map(|r| r.ts.clone()).collect();
        assert_eq!(ts_list, vec!["t2000-b".to_string(), "t3000".to_string()]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_jump_to_time_inclusive() {
        let (path, executor) = seed_db();
        let filters = LogFilters::default();

        let resp = executor
            .query_from_time(&filters, 2000.0, 10)
            .unwrap();

        let ts_list: Vec<String> = resp.rows.iter().map(|r| r.ts.clone()).collect();
        assert_eq!(ts_list, vec!["t2000-a".to_string(), "t2000-b".to_string(), "t3000".to_string()]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_invalid_not_text_regex_returns_error() {
        let (path, executor) = seed_db();
        let mut filters = LogFilters::default();
        filters.text_mode = Some("regex".to_string());
        filters.not_text = Some("(".to_string());

        let err = executor
            .query(&filters, None, 10, CursorDirection::Forward)
            .err()
            .expect("expected error");

        match err {
            LogcatError::InvalidFilter(_) => {}
            LogcatError::Regex(_) => {}
            other => panic!("unexpected error: {:?}", other),
        }

        std::fs::remove_file(&path).ok();
    }
}
