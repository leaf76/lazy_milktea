use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use tauri::{State, Emitter};
use tauri::menu::{MenuItemBuilder, SubmenuBuilder, MenuBuilder};

mod error;
mod types;
mod time;
mod parser;
mod index;
mod query;

// Re-export for backward compatibility
pub use error::{LogcatError, Result};
pub use types::{DeviceInfo, LogRow, LogFilters};
pub use query::{QueryCursor, CursorDirection, QueryResponse, LogcatStats};

#[derive(Default)]
struct AppState {
    last_cache_dir: Option<PathBuf>,
}

// ============================================================================
// V1 API (kept for backward compatibility)
// ============================================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ParseSummary {
    device: types::DeviceInfo,
    events: usize,
    anrs: usize,
    crashes: usize,
    ef_total: usize,
    ef_recent: usize,
}

#[tauri::command]
async fn parse_bugreport(path: String, state: State<'_, Mutex<AppState>>) -> std::result::Result<ParseSummary, String> {
    let result = parser::parse_bugreport(&path).map_err(|e| e.to_string())?;

    // Update state with cache directory
    if let Ok(mut guard) = state.lock() {
        guard.last_cache_dir.replace(result.cache_dir.clone());
    }

    Ok(ParseSummary {
        device: result.device,
        events: result.anr_count + result.crash_count,
        anrs: result.anr_count,
        crashes: result.crash_count,
        ef_total: result.index_summary.error_count + result.index_summary.fatal_count,
        ef_recent: result.index_summary.fatal_count,
    })
}

// Legacy query function (kept for compatibility)
#[tauri::command]
async fn query_logcat(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
    page: u32,
    page_size: u32,
) -> std::result::Result<Vec<types::LogRow>, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;

    let db_path = cache_dir.join("logcat.db");
    let executor = query::QueryExecutor::open(&db_path).map_err(|e| e.to_string())?;

    // Calculate offset based on page number (0-indexed)
    let offset = (page as i64) * (page_size as i64);
    let cursor = if offset > 0 {
        Some(QueryCursor::new(offset, CursorDirection::Forward, 0))
    } else {
        None
    };

    let response = executor
        .query(&filters, cursor.as_ref(), page_size as usize, CursorDirection::Forward)
        .map_err(|e| e.to_string())?;

    Ok(response.rows)
}

// Legacy stream function (kept for compatibility)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamResp {
    rows: Vec<types::LogRow>,
    next_cursor: u64,
    exhausted: bool,
    file_size: u64,
    total_rows: Option<usize>,
    min_iso_ms: Option<u64>,
    max_iso_ms: Option<u64>,
}

#[tauri::command]
async fn query_logcat_stream(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
    cursor: Option<u64>,
    limit: u32,
) -> std::result::Result<StreamResp, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;

    let db_path = cache_dir.join("logcat.db");
    let executor = query::QueryExecutor::open(&db_path).map_err(|e| e.to_string())?;

    // Convert legacy cursor to new format
    let query_cursor = cursor.map(|pos| QueryCursor::new(pos as i64, CursorDirection::Forward, 0));

    let response = executor
        .query(&filters, query_cursor.as_ref(), limit as usize, CursorDirection::Forward)
        .map_err(|e| e.to_string())?;

    let stats = executor.get_stats(&filters).map_err(|e| e.to_string())?;

    Ok(StreamResp {
        rows: response.rows,
        next_cursor: response.next_cursor.map(|c| c.position as u64).unwrap_or(0),
        exhausted: !response.has_more_next,
        file_size: 0, // Not applicable for SQLite
        total_rows: Some(stats.total_rows),
        min_iso_ms: stats.min_timestamp_ms.map(|t| t as u64),
        max_iso_ms: stats.max_timestamp_ms.map(|t| t as u64),
    })
}

// ============================================================================
// V2 API (new cursor-based API)
// ============================================================================

#[tauri::command]
async fn query_logcat_v2(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
    cursor: Option<QueryCursor>,
    limit: u32,
    direction: Option<CursorDirection>,
) -> std::result::Result<QueryResponse, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;

    let db_path = cache_dir.join("logcat.db");
    let executor = query::QueryExecutor::open(&db_path).map_err(|e| e.to_string())?;

    let dir = direction.unwrap_or(CursorDirection::Forward);

    executor
        .query(&filters, cursor.as_ref(), limit as usize, dir)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn jump_to_time(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
    target_time: String,
    limit: u32,
) -> std::result::Result<QueryResponse, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;

    let db_path = cache_dir.join("logcat.db");
    let executor = query::QueryExecutor::open(&db_path).map_err(|e| e.to_string())?;

    // Convert target time to filters
    let mut time_filters = filters.clone();
    time_filters.ts_from = Some(target_time);

    executor
        .query(&time_filters, None, limit as usize, CursorDirection::Forward)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_logcat_stats(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
) -> std::result::Result<LogcatStats, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;

    let db_path = cache_dir.join("logcat.db");
    let executor = query::QueryExecutor::open(&db_path).map_err(|e| e.to_string())?;

    executor.get_stats(&filters).map_err(|e| e.to_string())
}

// ============================================================================
// Streaming Parse API (for large files)
// ============================================================================

/// Progress event payload for frontend
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ParseProgress {
    bytes_read: u64,
    total_bytes: u64,
    rows_processed: usize,
    phase: String,
    percent: f32,
}

#[tauri::command]
async fn parse_bugreport_streaming(
    app: tauri::AppHandle,
    path: String,
    state: State<'_, Mutex<AppState>>,
) -> std::result::Result<ParseSummary, String> {
    use index::IndexPhase;

    let app_clone = app.clone();

    let result = parser::parse_bugreport_streaming(&path, move |progress| {
        let phase = match progress.phase {
            IndexPhase::Parsing => "parsing",
            IndexPhase::BuildingFts => "building_fts",
            IndexPhase::Optimizing => "optimizing",
            IndexPhase::Complete => "complete",
        };

        let percent = if progress.total_bytes > 0 {
            (progress.bytes_read as f32 / progress.total_bytes as f32) * 100.0
        } else {
            0.0
        };

        let payload = ParseProgress {
            bytes_read: progress.bytes_read,
            total_bytes: progress.total_bytes,
            rows_processed: progress.rows_processed,
            phase: phase.to_string(),
            percent,
        };

        let _ = app_clone.emit("parse://progress", payload);
    }).map_err(|e| e.to_string())?;

    // Update state with cache directory
    if let Ok(mut guard) = state.lock() {
        guard.last_cache_dir.replace(result.cache_dir.clone());
    }

    Ok(ParseSummary {
        device: result.device,
        events: result.anr_count + result.crash_count,
        anrs: result.anr_count,
        crashes: result.crash_count,
        ef_total: result.index_summary.error_count + result.index_summary.fatal_count,
        ef_recent: result.index_summary.fatal_count,
    })
}

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .menu(|app| {
            let file_menu = SubmenuBuilder::new(app, "File")
                .item(&MenuItemBuilder::new("Open…").id("open").accelerator("CmdOrCtrl+O").build(app)?)
                .build()?;
            let view_menu = SubmenuBuilder::new(app, "View")
                .item(&MenuItemBuilder::new("Dashboard").id("nav_dashboard").accelerator("CmdOrCtrl+1").build(app)?)
                .item(&MenuItemBuilder::new("Logcat").id("nav_logcat").accelerator("CmdOrCtrl+2").build(app)?)
                .item(&MenuItemBuilder::new("Timeline").id("nav_timeline").accelerator("CmdOrCtrl+3").build(app)?)
                .build()?;
            MenuBuilder::new(app).items(&[&file_menu, &view_menu]).build()
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open" => { let _ = app.emit("menu://open", ()); }
                "nav_dashboard" => { let _ = app.emit("nav://dashboard", ()); }
                "nav_logcat" => { let _ = app.emit("nav://logcat", ()); }
                "nav_timeline" => { let _ = app.emit("nav://timeline", ()); }
                _ => {}
            }
        })
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            // V1 API (backward compatible)
            parse_bugreport,
            query_logcat,
            query_logcat_stream,
            // V2 API (new)
            query_logcat_v2,
            jump_to_time,
            get_logcat_stats,
            // Streaming API (for large files)
            parse_bugreport_streaming,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
