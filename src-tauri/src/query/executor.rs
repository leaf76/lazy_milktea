use crate::error::{LogcatError, Result};
use crate::query::cursor::{QueryCursor, QueryResponse, CursorDirection, LogcatStats, LevelCounts};
use crate::query::filter::{compile_user_regex, plain_text_contains};
use crate::types::{LogFilters, LogRow};
use rusqlite::Connection;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Query executor for SQLite-based logcat index
pub struct QueryExecutor {
    conn: Connection,
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
        if let Some(c) = cursor {
            if c.filter_hash != filter_hash && c.filter_hash != 0 {
                return Err(LogcatError::InvalidFilter(
                    "Filter changed, cursor invalid".to_string()
                ));
            }
        }

        // Build and execute query
        let rows = self.execute_query(filters, cursor, limit, direction)?;

        // Build response
        let has_more = rows.len() >= limit;

        // Get last row id for next cursor
        let last_id = if !rows.is_empty() {
            // Query the max id from our result set
            cursor.map(|c| c.position).unwrap_or(0) + rows.len() as i64
        } else {
            cursor.map(|c| c.position).unwrap_or(0)
        };

        let first_id = cursor.map(|c| c.position).unwrap_or(0);

        Ok(QueryResponse {
            rows,
            next_cursor: if has_more {
                Some(QueryCursor::new(last_id, CursorDirection::Forward, filter_hash))
            } else {
                None
            },
            prev_cursor: if first_id > 0 {
                Some(QueryCursor::new(first_id, CursorDirection::Backward, filter_hash))
            } else {
                None
            },
            has_more_next: has_more && matches!(direction, CursorDirection::Forward),
            has_more_prev: cursor.is_some() && first_id > 0,
            estimated_total: None,
            position_ratio: 0.0,
        })
    }

    fn execute_query(
        &self,
        filters: &LogFilters,
        cursor: Option<&QueryCursor>,
        limit: usize,
        direction: CursorDirection,
    ) -> Result<Vec<LogRow>> {
        // Build WHERE conditions with parameterized queries
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

        // Tag filter (parameterized substring match)
        if let Some(ref tag) = filters.tag {
            conditions.push("tag LIKE ?".to_string());
            params.push(Box::new(format!("%{}%", tag)));
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
        if let Some(ref ts_from) = filters.ts_from {
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

        // Cursor position (parameterized)
        if let Some(c) = cursor {
            match direction {
                CursorDirection::Forward => {
                    conditions.push("id > ?".to_string());
                    params.push(Box::new(c.position));
                }
                CursorDirection::Backward => {
                    conditions.push("id < ?".to_string());
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

        let order = match direction {
            CursorDirection::Forward => "ASC",
            CursorDirection::Backward => "DESC",
        };

        let sql = format!(
            "SELECT id, ts_display, ts_iso, level, tag, pid, tid, msg FROM logs {} ORDER BY ts_unix {}, id {} LIMIT ?",
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

        let rows: Vec<LogRow> = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(LogRow {
                    ts: row.get(1)?,
                    ts_iso: row.get(2)?,
                    level: row.get(3)?,
                    tag: row.get(4)?,
                    pid: row.get(5)?,
                    tid: row.get(6)?,
                    msg: row.get(7)?,
                })
            })
            .map_err(|e| LogcatError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        // Apply text filter in Rust if using regex
        let rows = self.apply_text_filters(rows, filters)?;

        Ok(rows)
    }

    fn apply_text_filters(&self, rows: Vec<LogRow>, filters: &LogFilters) -> Result<Vec<LogRow>> {
        let mode = filters.text_mode.as_deref().unwrap_or("plain");
        let case_sensitive = filters.case_sensitive.unwrap_or(false);

        // Apply include text filter
        let rows = if let Some(ref text) = filters.text {
            if mode == "regex" {
                let re = compile_user_regex(text, !case_sensitive)?;
                rows.into_iter()
                    .filter(|r| re.is_match(&r.msg))
                    .collect()
            } else {
                rows.into_iter()
                    .filter(|r| plain_text_contains(&r.msg, text, case_sensitive))
                    .collect()
            }
        } else {
            rows
        };

        // Apply exclude text filter
        let rows = if let Some(ref not_text) = filters.not_text {
            if mode == "regex" {
                if let Ok(re) = compile_user_regex(not_text, !case_sensitive) {
                    rows.into_iter()
                        .filter(|r| !re.is_match(&r.msg))
                        .collect()
                } else {
                    rows
                }
            } else {
                rows.into_iter()
                    .filter(|r| !plain_text_contains(&r.msg, not_text, case_sensitive))
                    .collect()
            }
        } else {
            rows
        };

        Ok(rows)
    }

    /// Get statistics about the logcat data
    pub fn get_stats(&self, _filters: &LogFilters) -> Result<LogcatStats> {
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

        Ok(LogcatStats {
            total_rows,
            filtered_rows: None,
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
}

/// Compute hash of filter conditions for cursor validation
fn compute_filter_hash(filters: &LogFilters) -> u64 {
    let mut hasher = DefaultHasher::new();
    filters.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
