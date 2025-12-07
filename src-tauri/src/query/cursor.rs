use serde::{Deserialize, Serialize};
use crate::types::LogRow;

/// Direction for cursor-based queries
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CursorDirection {
    #[default]
    Forward,   // Scrolling down (time increasing)
    Backward,  // Scrolling up (time decreasing)
}

/// Cursor for paginated queries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCursor {
    /// Row ID (SQLite rowid) at current position
    pub position: i64,
    /// Query direction
    pub direction: CursorDirection,
    /// Hash of filter conditions (to validate cursor)
    pub filter_hash: u64,
}

impl QueryCursor {
    pub fn new(position: i64, direction: CursorDirection, filter_hash: u64) -> Self {
        Self {
            position,
            direction,
            filter_hash,
        }
    }

    /// Create a forward cursor starting from the beginning
    pub fn start(filter_hash: u64) -> Self {
        Self {
            position: 0,
            direction: CursorDirection::Forward,
            filter_hash,
        }
    }
}

/// Response for cursor-based queries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    /// Log rows for this page
    pub rows: Vec<LogRow>,
    /// Cursor for fetching next page (forward)
    pub next_cursor: Option<QueryCursor>,
    /// Cursor for fetching previous page (backward)
    pub prev_cursor: Option<QueryCursor>,
    /// Whether there are more rows after this page
    pub has_more_next: bool,
    /// Whether there are more rows before this page
    pub has_more_prev: bool,
    /// Estimated total rows matching the filter
    pub estimated_total: Option<usize>,
    /// Current position ratio (0.0 - 1.0) for progress bar
    pub position_ratio: f32,
}

impl Default for QueryResponse {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            next_cursor: None,
            prev_cursor: None,
            has_more_next: false,
            has_more_prev: false,
            estimated_total: None,
            position_ratio: 0.0,
        }
    }
}

/// Statistics about logcat data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogcatStats {
    /// Total rows in database
    pub total_rows: usize,
    /// Rows matching current filter
    pub filtered_rows: Option<usize>,
    /// Earliest timestamp (Unix ms)
    pub min_timestamp_ms: Option<i64>,
    /// Latest timestamp (Unix ms)
    pub max_timestamp_ms: Option<i64>,
    /// Earliest timestamp display string (device local time)
    pub min_ts_display: Option<String>,
    /// Latest timestamp display string (device local time)
    pub max_ts_display: Option<String>,
    /// Count by log level
    pub level_counts: LevelCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelCounts {
    pub verbose: usize,
    pub debug: usize,
    pub info: usize,
    pub warning: usize,
    pub error: usize,
    pub fatal: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_serialization() {
        let cursor = QueryCursor::new(100, CursorDirection::Forward, 12345);
        let json = serde_json::to_string(&cursor).unwrap();
        assert!(json.contains("\"position\":100"));
        assert!(json.contains("\"direction\":\"forward\""));

        let parsed: QueryCursor = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.position, 100);
    }

    #[test]
    fn test_response_default() {
        let resp = QueryResponse::default();
        assert!(resp.rows.is_empty());
        assert!(!resp.has_more_next);
    }
}
