use crate::error::{LogcatError, Result};
use crate::parser::device::extract_device_info;
use crate::index::{IndexBuilder, IndexSummary, StreamingIndexBuilder, IndexProgress};
use crate::types::DeviceInfo;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Arc;
use zip::read::ZipArchive;
use dirs::home_dir;

/// Result of parsing a bugreport
#[derive(Debug)]
pub struct ParseResult {
    pub device: DeviceInfo,
    pub anr_count: usize,
    pub crash_count: usize,
    pub index_summary: IndexSummary,
    pub cache_dir: std::path::PathBuf,
}

/// Parse a bugreport file (zip or txt)
pub fn parse_bugreport(path: &str) -> Result<ParseResult> {
    let cache_dir = prepare_cache_dir(path)?;
    let db_path = cache_dir.join("logcat.db");

    if is_zip(path) {
        parse_zip(path, &cache_dir, &db_path)
    } else {
        parse_txt(path, &cache_dir, &db_path)
    }
}

fn is_zip(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".zip")
}

/// Prepare cache directory for parsed data
fn prepare_cache_dir(report_path: &str) -> Result<std::path::PathBuf> {
    let home = home_dir()
        .ok_or_else(|| LogcatError::CacheNotFound("cannot find home dir".to_string()))?;

    let name = Path::new(report_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("report");

    let dir = home.join(".lazy_milktea_cache").join(name);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn parse_zip(path: &str, cache_dir: &Path, db_path: &Path) -> Result<ParseResult> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| LogcatError::Zip(e))?;

    // Find the main bugreport txt
    let mut chosen_index: Option<usize> = None;
    let mut chosen_size: u64 = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i)
            .map_err(|e| LogcatError::Zip(e))?;
        let name = file.name().to_string();
        let lower = name.to_ascii_lowercase();

        if lower.ends_with(".txt")
            && (lower.contains("bugreport") || lower.contains("main_entry"))
        {
            let size = file.size();
            if size > chosen_size {
                chosen_index = Some(i);
                chosen_size = size;
            }
        }
    }

    let idx = chosen_index.ok_or(LogcatError::NoBugreportFound)?;
    let mut file = archive.by_index(idx)
        .map_err(|e| LogcatError::Zip(e))?;

    // Read content
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // Extract device info
    let (device, anr_count, crash_count) = extract_device_info(&content);

    // Build logcat index
    let index_summary = IndexBuilder::new(db_path)?
        .build_from_text(&content)?;

    Ok(ParseResult {
        device,
        anr_count,
        crash_count,
        index_summary,
        cache_dir: cache_dir.to_path_buf(),
    })
}

fn parse_txt(path: &str, cache_dir: &Path, db_path: &Path) -> Result<ParseResult> {
    let content = std::fs::read_to_string(path)?;

    // Extract device info
    let (device, anr_count, crash_count) = extract_device_info(&content);

    // Build logcat index
    let index_summary = IndexBuilder::new(db_path)?
        .build_from_text(&content)?;

    Ok(ParseResult {
        device,
        anr_count,
        crash_count,
        index_summary,
        cache_dir: cache_dir.to_path_buf(),
    })
}

/// Get the cache directory path for a report
pub fn get_cache_dir(report_path: &str) -> Result<std::path::PathBuf> {
    prepare_cache_dir(report_path)
}

// ============================================================================
// Streaming Parser for Large Files
// ============================================================================

/// Progress callback for streaming parsing
pub type ProgressCallback = Arc<dyn Fn(IndexProgress) + Send + Sync>;

/// Parse a bugreport with streaming (for large files)
pub fn parse_bugreport_streaming<F>(
    path: &str,
    progress: F,
) -> Result<ParseResult>
where
    F: Fn(IndexProgress) + Send + Sync + 'static,
{
    let cache_dir = prepare_cache_dir(path)?;
    let db_path = cache_dir.join("logcat.db");

    if is_zip(path) {
        parse_zip_streaming(path, &cache_dir, &db_path, progress)
    } else {
        parse_txt_streaming(path, &cache_dir, &db_path, progress)
    }
}

fn parse_txt_streaming<F>(
    path: &str,
    cache_dir: &Path,
    db_path: &Path,
    progress: F,
) -> Result<ParseResult>
where
    F: Fn(IndexProgress) + Send + Sync + 'static,
{
    let file = File::open(path)?;
    let file_size = file.metadata()?.len();

    // Read a sample for device info (first 256KB)
    let mut sample_reader = BufReader::new(File::open(path)?);
    let mut sample = vec![0u8; (256 * 1024).min(file_size as usize)];
    let bytes_read = sample_reader.read(&mut sample)?;
    sample.truncate(bytes_read);
    let sample_str = String::from_utf8_lossy(&sample);
    let (device, anr_count, crash_count) = extract_device_info(&sample_str);
    drop(sample_reader);

    // Build index with streaming
    let index_summary = StreamingIndexBuilder::new(db_path)
        .with_progress(progress)
        .build_from_file(Path::new(path))?;

    Ok(ParseResult {
        device,
        anr_count,
        crash_count,
        index_summary: IndexSummary {
            total_rows: index_summary.total_rows,
            error_count: index_summary.error_count,
            fatal_count: index_summary.fatal_count,
            min_timestamp_ms: index_summary.min_timestamp_ms,
            max_timestamp_ms: index_summary.max_timestamp_ms,
        },
        cache_dir: cache_dir.to_path_buf(),
    })
}

fn parse_zip_streaming<F>(
    path: &str,
    cache_dir: &Path,
    db_path: &Path,
    progress: F,
) -> Result<ParseResult>
where
    F: Fn(IndexProgress) + Send + Sync + 'static,
{
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(|e| LogcatError::Zip(e))?;

    // Find the main bugreport txt
    let mut chosen_index: Option<usize> = None;
    let mut chosen_size: u64 = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| LogcatError::Zip(e))?;
        let name = file.name().to_string();
        let lower = name.to_ascii_lowercase();

        if lower.ends_with(".txt")
            && (lower.contains("bugreport") || lower.contains("main_entry"))
        {
            let size = file.size();
            if size > chosen_size {
                chosen_index = Some(i);
                chosen_size = size;
            }
        }
    }

    let idx = chosen_index.ok_or(LogcatError::NoBugreportFound)?;

    // For zip files, we need to extract to a temp file first
    // because ZipFile doesn't support Seek well
    let temp_path = cache_dir.join("_temp_bugreport.txt");
    {
        let mut zip_file = archive.by_index(idx).map_err(|e| LogcatError::Zip(e))?;
        let mut temp_file = File::create(&temp_path)?;
        std::io::copy(&mut zip_file, &mut temp_file)?;
    }

    // Read sample for device info
    let mut sample_reader = BufReader::new(File::open(&temp_path)?);
    let mut sample = vec![0u8; (256 * 1024).min(chosen_size as usize)];
    let bytes_read = sample_reader.read(&mut sample)?;
    sample.truncate(bytes_read);
    let sample_str = String::from_utf8_lossy(&sample);
    let (device, anr_count, crash_count) = extract_device_info(&sample_str);
    drop(sample_reader);

    // Build index with streaming
    let index_summary = StreamingIndexBuilder::new(db_path)
        .with_progress(progress)
        .build_from_file(&temp_path)?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);

    Ok(ParseResult {
        device,
        anr_count,
        crash_count,
        index_summary: IndexSummary {
            total_rows: index_summary.total_rows,
            error_count: index_summary.error_count,
            fatal_count: index_summary.fatal_count,
            min_timestamp_ms: index_summary.min_timestamp_ms,
            max_timestamp_ms: index_summary.max_timestamp_ms,
        },
        cache_dir: cache_dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_zip() {
        assert!(is_zip("bugreport.zip"));
        assert!(is_zip("BUGREPORT.ZIP"));
        assert!(!is_zip("bugreport.txt"));
    }
}
