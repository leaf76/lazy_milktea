use crate::error::{LogcatError, Result};
use crate::types::LogRow;
use rusqlite::{Connection, params};
use std::path::Path;

/// SQLite-based logcat database
pub struct LogcatDatabase {
    conn: Connection,
}

impl LogcatDatabase {
    /// Create a new database at the specified path
    pub fn create(db_path: &Path) -> Result<Self> {
        // Remove existing database if present
        if db_path.exists() {
            std::fs::remove_file(db_path)
                .map_err(|e| LogcatError::Io(e))?;
        }

        let conn = Connection::open(db_path)
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        // Create schema
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -64000;  -- 64MB cache

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

            CREATE INDEX idx_ts ON logs(ts_unix);
            CREATE INDEX idx_level ON logs(level);
            CREATE INDEX idx_tag ON logs(tag);
            CREATE INDEX idx_pid ON logs(pid);

            CREATE VIRTUAL TABLE logs_fts USING fts5(
                msg,
                content=logs,
                content_rowid=id
            );

            CREATE TRIGGER logs_ai AFTER INSERT ON logs BEGIN
                INSERT INTO logs_fts(rowid, msg) VALUES (new.id, new.msg);
            END;
            "#,
        )
        .map_err(|e| LogcatError::Database(e.to_string()))?;

        Ok(Self { conn })
    }

    /// Open an existing database
    pub fn open(db_path: &Path) -> Result<Self> {
        if !db_path.exists() {
            return Err(LogcatError::CacheNotFound(
                db_path.display().to_string()
            ));
        }

        let conn = Connection::open(db_path)
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        Ok(Self { conn })
    }

    /// Begin a transaction for batch inserts
    pub fn begin_batch(&mut self) -> Result<BatchInserter<'_>> {
        self.conn
            .execute("BEGIN TRANSACTION", [])
            .map_err(|e| LogcatError::Database(e.to_string()))?;

        Ok(BatchInserter { db: self, committed: false })
    }

    /// Insert a single log row
    pub fn insert(&self, row: &LogRow, ts_unix_ms: f64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO logs (ts_unix, ts_display, ts_iso, level, tag, pid, tid, msg) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
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

    /// Get total row count
    pub fn count(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM logs", [], |r| r.get(0))
            .map_err(|e| LogcatError::Database(e.to_string()))
    }

    /// Get time range
    pub fn time_range(&self) -> Result<(Option<f64>, Option<f64>)> {
        self.conn
            .query_row(
                "SELECT MIN(ts_unix), MAX(ts_unix) FROM logs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| LogcatError::Database(e.to_string()))
    }

    /// Commit pending changes
    fn commit(&self) -> Result<()> {
        self.conn
            .execute("COMMIT", [])
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    /// Optimize after bulk inserts
    pub fn optimize(&self) -> Result<()> {
        self.conn
            .execute_batch(
                r#"
                INSERT INTO logs_fts(logs_fts) VALUES('optimize');
                ANALYZE;
                "#,
            )
            .map_err(|e| LogcatError::Database(e.to_string()))?;
        Ok(())
    }

    /// Get the underlying connection for advanced queries
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// Batch inserter for efficient bulk inserts
pub struct BatchInserter<'a> {
    db: &'a mut LogcatDatabase,
    committed: bool,
}

impl<'a> BatchInserter<'a> {
    /// Insert a row in the current transaction
    pub fn insert(&self, row: &LogRow, ts_unix_ms: f64) -> Result<()> {
        self.db.insert(row, ts_unix_ms)
    }

    /// Commit the batch
    pub fn commit(mut self) -> Result<()> {
        self.db.commit()?;
        self.committed = true;
        Ok(())
    }
}

impl<'a> Drop for BatchInserter<'a> {
    fn drop(&mut self) {
        // Rollback only if not committed
        if !self.committed {
            let _ = self.db.conn.execute("ROLLBACK", []);
        }
    }
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
        std::env::temp_dir().join(format!("test_logcat_{}.db", nanos))
    }

    #[test]
    fn test_create_and_insert() {
        let path = temp_db_path();
        let db = LogcatDatabase::create(&path).unwrap();

        let row = LogRow {
            ts: "08-24 14:22:33.123".to_string(),
            ts_iso: Some("2024-08-24T06:22:33.123+00:00".to_string()),
            level: "E".to_string(),
            tag: "ActivityManager".to_string(),
            pid: 1234,
            tid: 5678,
            msg: "ANR in com.example".to_string(),
        };

        db.insert(&row, 1724487753123.0).unwrap();

        assert_eq!(db.count().unwrap(), 1);

        // Cleanup
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_batch_insert() {
        let path = temp_db_path();
        let mut db = LogcatDatabase::create(&path).unwrap();

        {
            let batch = db.begin_batch().unwrap();
            for i in 0..100 {
                let row = LogRow {
                    ts: format!("08-24 14:22:{:02}.000", i % 60),
                    ts_iso: None,
                    level: "I".to_string(),
                    tag: "Test".to_string(),
                    pid: 1000,
                    tid: 1000,
                    msg: format!("Message {}", i),
                };
                batch.insert(&row, 1724487753000.0 + i as f64 * 1000.0).unwrap();
            }
            batch.commit().unwrap();
        }

        assert_eq!(db.count().unwrap(), 100);

        // Cleanup
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_fts_search() {
        let path = temp_db_path();
        let db = LogcatDatabase::create(&path).unwrap();

        let row = LogRow {
            ts: "08-24 14:22:33.123".to_string(),
            ts_iso: None,
            level: "E".to_string(),
            tag: "Test".to_string(),
            pid: 1000,
            tid: 1000,
            msg: "Hello world from Android".to_string(),
        };
        db.insert(&row, 1724487753123.0).unwrap();

        // FTS search
        let count: i64 = db.connection()
            .query_row(
                "SELECT COUNT(*) FROM logs WHERE id IN (SELECT rowid FROM logs_fts WHERE logs_fts MATCH 'android')",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(count, 1);

        // Cleanup
        std::fs::remove_file(&path).ok();
    }
}
