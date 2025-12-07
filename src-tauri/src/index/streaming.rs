use crate::error::{LogcatError, Result};
use crate::index::sqlite::LogcatDatabase;
use crate::parser::LOGCAT_RE;
use crate::time::{TimeAnchor, derive_time_anchor, to_iso_safe, iso_ts_key_ms};
use crate::types::LogRow;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Batch size for commits (rows per transaction)
const BATCH_COMMIT_SIZE: usize = 50_000;

/// Size of buffer for reading file
const READ_BUFFER_SIZE: usize = 64 * 1024; // 64KB

/// Size to sample for time anchor detection
const ANCHOR_SAMPLE_SIZE: usize = 256 * 1024; // 256KB

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(IndexProgress) + Send + Sync>;

/// Index building progress
#[derive(Debug, Clone)]
pub struct IndexProgress {
    pub bytes_read: u64,
    pub total_bytes: u64,
    pub rows_processed: usize,
    pub phase: IndexPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexPhase {
    Parsing,
    BuildingFts,
    Optimizing,
    Complete,
}

/// Summary of index building results
#[derive(Debug, Clone, Default)]
pub struct IndexSummary {
    pub total_rows: usize,
    pub error_count: usize,
    pub fatal_count: usize,
    pub min_timestamp_ms: Option<u64>,
    pub max_timestamp_ms: Option<u64>,
}

/// Streaming index builder for large files
pub struct StreamingIndexBuilder {
    db_path: std::path::PathBuf,
    anchor: Option<TimeAnchor>,
    progress_callback: Option<ProgressCallback>,
    cancel_flag: Arc<AtomicBool>,
}

impl StreamingIndexBuilder {
    /// Create a new streaming index builder
    pub fn new(db_path: &Path) -> Self {
        Self {
            db_path: db_path.to_path_buf(),
            anchor: None,
            progress_callback: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set the time anchor
    pub fn with_anchor(mut self, anchor: TimeAnchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Set progress callback
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(IndexProgress) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Get cancel flag for external cancellation
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }

    /// Build index from a file path using streaming
    pub fn build_from_file(self, file_path: &Path) -> Result<IndexSummary> {
        let file = std::fs::File::open(file_path)
            .map_err(|e| LogcatError::Io(e))?;

        let total_bytes = file.metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        self.build_from_reader(file, total_bytes)
    }

    /// Build index from any reader
    pub fn build_from_reader<R: Read + Seek>(mut self, mut reader: R, total_bytes: u64) -> Result<IndexSummary> {
        // Step 1: Sample beginning for time anchor
        let anchor = match self.anchor.take() {
            Some(a) => a,
            None => {
                let mut sample = vec![0u8; ANCHOR_SAMPLE_SIZE.min(total_bytes as usize)];
                let bytes_read = reader.read(&mut sample)
                    .map_err(|e| LogcatError::Io(e))?;
                sample.truncate(bytes_read);
                reader.seek(SeekFrom::Start(0))
                    .map_err(|e| LogcatError::Io(e))?;

                let sample_str = String::from_utf8_lossy(&sample);
                derive_time_anchor(&sample_str)
            }
        };

        // Step 2: Create database (disable FTS trigger for bulk loading)
        let mut db = self.create_db_without_fts_trigger()?;

        let mut summary = IndexSummary::default();
        let mut bytes_read: u64 = 0;
        let mut batch_count = 0;

        // Step 3: Stream parse with batched commits
        let buf_reader = BufReader::with_capacity(READ_BUFFER_SIZE, reader);

        db.begin_transaction()?;

        for line_result in buf_reader.lines() {
            // Check cancellation
            if self.cancel_flag.load(Ordering::Relaxed) {
                db.rollback()?;
                return Err(LogcatError::InvalidFilter("Index building cancelled".to_string()));
            }

            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue, // Skip invalid UTF-8 lines
            };

            bytes_read += line.len() as u64 + 1; // +1 for newline

            // Try to parse as logcat line
            if let Some(caps) = LOGCAT_RE.captures(&line) {
                let ts = format!("{} {}", &caps["date"], &caps["time"]);
                let level = caps["level"].to_string();

                let ts_iso = to_iso_safe(&ts, &anchor).ok();
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

                db.insert_row(&row, ts_unix_ms)?;

                // Update summary
                summary.total_rows += 1;
                batch_count += 1;

                match level.as_str() {
                    "E" => summary.error_count += 1,
                    "F" => summary.fatal_count += 1,
                    _ => {}
                }

                if ts_unix_ms > 0.0 {
                    let ms = ts_unix_ms as u64;
                    summary.min_timestamp_ms = Some(
                        summary.min_timestamp_ms.map_or(ms, |m| m.min(ms))
                    );
                    summary.max_timestamp_ms = Some(
                        summary.max_timestamp_ms.map_or(ms, |m| m.max(ms))
                    );
                }

                // Commit batch periodically
                if batch_count >= BATCH_COMMIT_SIZE {
                    db.commit()?;
                    db.begin_transaction()?;
                    batch_count = 0;

                    // Report progress
                    if let Some(ref cb) = self.progress_callback {
                        cb(IndexProgress {
                            bytes_read,
                            total_bytes,
                            rows_processed: summary.total_rows,
                            phase: IndexPhase::Parsing,
                        });
                    }
                }
            }
        }

        // Commit final batch
        if batch_count > 0 {
            db.commit()?;
        }

        // Step 4: Build FTS index in batch
        if let Some(ref cb) = self.progress_callback {
            cb(IndexProgress {
                bytes_read: total_bytes,
                total_bytes,
                rows_processed: summary.total_rows,
                phase: IndexPhase::BuildingFts,
            });
        }

        db.rebuild_fts_index()?;

        // Step 5: Optimize
        if let Some(ref cb) = self.progress_callback {
            cb(IndexProgress {
                bytes_read: total_bytes,
                total_bytes,
                rows_processed: summary.total_rows,
                phase: IndexPhase::Optimizing,
            });
        }

        db.optimize()?;

        // Done
        if let Some(ref cb) = self.progress_callback {
            cb(IndexProgress {
                bytes_read: total_bytes,
                total_bytes,
                rows_processed: summary.total_rows,
                phase: IndexPhase::Complete,
            });
        }

        Ok(summary)
    }

    fn create_db_without_fts_trigger(&self) -> Result<StreamingDatabase> {
        // Remove existing database
        if self.db_path.exists() {
            std::fs::remove_file(&self.db_path)
                .map_err(|e| LogcatError::Io(e))?;
        }

        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        // Optimized settings for bulk insert
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = OFF;
            PRAGMA cache_size = -128000;  -- 128MB cache
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 268435456; -- 256MB mmap

            CREATE TABLE logs (
                id INTEGER PRIMARY KEY,
                ts_unix REAL NOT NULL,
                ts_display TEXT NOT NULL,
                ts_iso TEXT,
                level TEXT NOT NULL,
                tag TEXT NOT NULL,
                pid INTEGER NOT NULL,
                tid INTEGER NOT NULL,
                msg TEXT NOT NULL
            );

            -- Defer index creation for faster inserts
            "#,
        )
        .map_err(|e| LogcatError::Database(e.to_string()))?;

        Ok(StreamingDatabase { conn })
    }
}

/// Internal database wrapper for streaming
struct StreamingDatabase {
    conn: rusqlite::Connection,
}

impl StreamingDatabase {
    fn begin_transaction(&self) -> Result<()> {
        self.conn.execute("BEGIN TRANSACTION", [])
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    fn commit(&self) -> Result<()> {
        self.conn.execute("COMMIT", [])
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    fn rollback(&self) -> Result<()> {
        self.conn.execute("ROLLBACK", [])
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    fn insert_row(&self, row: &LogRow, ts_unix_ms: f64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO logs (ts_unix, ts_display, ts_iso, level, tag, pid, tid, msg) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    ts_unix_ms,
                    row.ts,
                    row.ts_iso,
                    row.level,
                    row.tag,
                    row.pid,
                    row.tid,
                    row.msg,
                ],
            )
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    fn rebuild_fts_index(&self) -> Result<()> {
        // Create indexes now that data is loaded
        self.conn.execute_batch(
            r#"
            CREATE INDEX idx_ts ON logs(ts_unix);
            CREATE INDEX idx_level ON logs(level);
            CREATE INDEX idx_tag ON logs(tag);
            CREATE INDEX idx_pid ON logs(pid);

            -- Create FTS table and populate in one go
            CREATE VIRTUAL TABLE logs_fts USING fts5(
                msg,
                content=logs,
                content_rowid=id
            );

            INSERT INTO logs_fts(logs_fts) VALUES('rebuild');
            "#,
        )
        .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    fn optimize(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA synchronous = NORMAL;
            INSERT INTO logs_fts(logs_fts) VALUES('optimize');
            ANALYZE;
            "#,
        )
        .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("test_streaming_{}.db", nanos))
    }

    #[test]
    fn test_streaming_small() {
        let sample = r#"persist.sys.timezone=Asia/Taipei
08-24 14:22:33.123  1234  5678 E ActivityManager: ANR in com.foo
08-24 14:22:34.999  1234  5678 I MyTag: hello world
08-24 14:22:35.001  2222  5679 W Network: unstable
"#;

        let db_path = temp_db_path();
        let cursor = Cursor::new(sample.as_bytes().to_vec());

        let summary = StreamingIndexBuilder::new(&db_path)
            .build_from_reader(cursor, sample.len() as u64)
            .unwrap();

        assert_eq!(summary.total_rows, 3);
        assert_eq!(summary.error_count, 1);

        // Verify database
        let db = LogcatDatabase::open(&db_path).unwrap();
        assert_eq!(db.count().unwrap(), 3);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn test_streaming_with_progress() {
        let sample = r#"08-24 14:22:33.123  1234  5678 E Test: error
08-24 14:22:34.000  1234  5678 I Test: info
"#;

        let db_path = temp_db_path();
        let cursor = Cursor::new(sample.as_bytes().to_vec());

        let progress_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let progress_count_clone = Arc::clone(&progress_count);

        let summary = StreamingIndexBuilder::new(&db_path)
            .with_progress(move |_p| {
                progress_count_clone.fetch_add(1, Ordering::Relaxed);
            })
            .build_from_reader(cursor, sample.len() as u64)
            .unwrap();

        assert_eq!(summary.total_rows, 2);
        // Should have progress callbacks for FTS building, optimizing, and complete
        assert!(progress_count.load(Ordering::Relaxed) >= 2);

        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn test_streaming_cancellation() {
        let sample = "08-24 14:22:33.123  1234  5678 E Test: error\n".repeat(100);

        let db_path = temp_db_path();
        let cursor = Cursor::new(sample.as_bytes().to_vec());

        let builder = StreamingIndexBuilder::new(&db_path);
        let cancel_flag = builder.cancel_flag();

        // Cancel immediately
        cancel_flag.store(true, Ordering::Relaxed);

        let result = builder.build_from_reader(cursor, sample.len() as u64);
        assert!(result.is_err());

        std::fs::remove_file(&db_path).ok();
    }
}
