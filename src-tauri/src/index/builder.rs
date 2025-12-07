use crate::error::{LogcatError, Result};
use crate::index::sqlite::LogcatDatabase;
use crate::parser::LOGCAT_RE_MULTILINE;
use crate::time::{TimeAnchor, derive_time_anchor, to_iso_safe, iso_ts_key_ms};
use crate::types::LogRow;
use std::path::Path;

/// Summary of index building results
#[derive(Debug, Clone, Default)]
pub struct IndexSummary {
    pub total_rows: usize,
    pub error_count: usize,
    pub fatal_count: usize,
    pub min_timestamp_ms: Option<u64>,
    pub max_timestamp_ms: Option<u64>,
}

/// Index builder for creating SQLite logcat database
pub struct IndexBuilder {
    db: LogcatDatabase,
    anchor: Option<TimeAnchor>,
    summary: IndexSummary,
}

impl IndexBuilder {
    /// Create a new index builder
    pub fn new(db_path: &Path) -> Result<Self> {
        let db = LogcatDatabase::create(db_path)?;
        Ok(Self {
            db,
            anchor: None,
            summary: IndexSummary::default(),
        })
    }

    /// Set the time anchor for timestamp conversion
    pub fn with_anchor(mut self, anchor: TimeAnchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Build index from text content
    pub fn build_from_text(mut self, text: &str) -> Result<IndexSummary> {
        // Derive time anchor if not set
        let anchor = self.anchor.take().unwrap_or_else(|| derive_time_anchor(text));

        // Begin batch insert
        let batch = self.db.begin_batch()?;

        for caps in LOGCAT_RE_MULTILINE.captures_iter(text) {
            let ts = format!("{} {}", &caps["date"], &caps["time"]);
            let level = caps["level"].to_string();

            // Convert to ISO timestamp
            let ts_iso = to_iso_safe(&ts, &anchor).ok();

            // Compute Unix timestamp for indexing
            let ts_unix_ms = ts_iso
                .as_ref()
                .and_then(|iso| iso_ts_key_ms(iso).ok())
                .unwrap_or(0) as f64;

            let row = LogRow {
                ts: ts.clone(),
                ts_iso,
                level: level.clone(),
                tag: caps["tag"].to_string(),
                pid: caps["pid"].parse().unwrap_or_default(),
                tid: caps["tid"].parse().unwrap_or_default(),
                msg: caps["msg"].to_string(),
            };

            batch.insert(&row, ts_unix_ms)?;

            // Update summary
            self.summary.total_rows += 1;
            match level.as_str() {
                "E" => self.summary.error_count += 1,
                "F" => self.summary.fatal_count += 1,
                _ => {}
            }

            // Update time range
            if ts_unix_ms > 0.0 {
                let ms = ts_unix_ms as u64;
                self.summary.min_timestamp_ms = Some(
                    self.summary.min_timestamp_ms.map_or(ms, |m| m.min(ms))
                );
                self.summary.max_timestamp_ms = Some(
                    self.summary.max_timestamp_ms.map_or(ms, |m| m.max(ms))
                );
            }
        }

        // Commit batch
        batch.commit()?;

        // Optimize database
        self.db.optimize()?;

        Ok(self.summary)
    }

    /// Build index from a file
    pub fn build_from_file(self, file_path: &Path) -> Result<IndexSummary> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| LogcatError::Io(e))?;
        self.build_from_text(&content)
    }
}

/// Quick function to build index from text
pub fn build_logcat_index(text: &str, db_path: &Path) -> Result<IndexSummary> {
    IndexBuilder::new(db_path)?.build_from_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("test_builder_{}.db", nanos))
    }

    #[test]
    fn test_build_from_text() {
        let sample = r#"
persist.sys.timezone=Asia/Taipei
08-24 14:22:33.123  1234  5678 E ActivityManager: ANR in com.foo
08-24 14:22:34.999  1234  5678 I MyTag: hello world
08-24 14:22:35.001  2222  5679 W Network: unstable
08-24 14:22:36.000  3333  5680 F Crash: fatal error
"#;

        let db_path = temp_db_path();
        let summary = IndexBuilder::new(&db_path)
            .unwrap()
            .build_from_text(sample)
            .unwrap();

        assert_eq!(summary.total_rows, 4);
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.fatal_count, 1);
        assert!(summary.min_timestamp_ms.is_some());
        assert!(summary.max_timestamp_ms.is_some());

        // Verify database content
        let db = LogcatDatabase::open(&db_path).unwrap();
        assert_eq!(db.count().unwrap(), 4);

        // Cleanup
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn test_fts_after_build() {
        let sample = r#"
08-24 14:22:33.123  1234  5678 E ActivityManager: ANR in com.example.app
08-24 14:22:34.000  1234  5678 I OtherTag: different message
"#;

        let db_path = temp_db_path();
        IndexBuilder::new(&db_path)
            .unwrap()
            .build_from_text(sample)
            .unwrap();

        let db = LogcatDatabase::open(&db_path).unwrap();

        // Search for "ANR"
        let count: i64 = db.connection()
            .query_row(
                "SELECT COUNT(*) FROM logs WHERE id IN (SELECT rowid FROM logs_fts WHERE logs_fts MATCH 'ANR')",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);

        // Cleanup
        std::fs::remove_file(&db_path).ok();
    }
}
