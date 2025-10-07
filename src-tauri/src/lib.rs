use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use tauri::{State, Emitter};
use tauri::menu::{Menu, MenuItemBuilder, SubmenuBuilder, MenuBuilder};

mod parser;
mod types;

#[derive(Default)]
struct AppState {
    last_cache_dir: Option<PathBuf>,
}

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
async fn parse_bugreport(path: String, state: State<'_, Mutex<AppState>>) -> Result<ParseSummary, String> {
    let res = parser::parse_entrypoint(&path).map_err(|e| e.to_string())?;
    let mut ef_total = 0usize;
    let mut ef_recent = 0usize;
    if let Ok(dir) = parser::prepare_cache_dir(&path) {
        if let Ok(sum) = parser::build_logcat_index(&path, &dir, None) {
            ef_total = sum.ef_total;
            ef_recent = sum.recent_ef;
        }
        if let Ok(mut guard) = state.lock() {
            guard.last_cache_dir.replace(dir);
        }
    }
    Ok(ParseSummary {
        device: res.device,
        events: res.anr_count + res.crash_count,
        anrs: res.anr_count,
        crashes: res.crash_count,
        ef_total,
        ef_recent,
    })
}

#[tauri::command]
async fn query_logcat(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
    page: u32,
    page_size: u32,
) -> Result<Vec<types::LogRow>, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;
    let rows = parser::query_logcat(&cache_dir, &filters, page as usize, page_size as usize)
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamResp { rows: Vec<types::LogRow>, next_cursor: u64, exhausted: bool, file_size: u64, total_rows: Option<usize> }

#[tauri::command]
async fn query_logcat_stream(
    state: State<'_, Mutex<AppState>>,
    filters: types::LogFilters,
    cursor: Option<u64>,
    limit: u32,
) -> Result<StreamResp, String> {
    let cache_dir = state
        .lock()
        .map_err(|_| "State poisoned".to_string())?
        .last_cache_dir
        .clone()
        .ok_or_else(|| "No cache yet. Please parse a bugreport first.".to_string())?;
    let res = parser::stream_logcat(&cache_dir, &filters, cursor, (limit as usize).max(1))
        .map_err(|e| e.to_string())?;
    Ok(StreamResp { rows: res.rows, next_cursor: res.next_cursor, exhausted: res.exhausted, file_size: res.file_size, total_rows: res.total_rows })
}

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
        .invoke_handler(tauri::generate_handler![parse_bugreport, query_logcat, query_logcat_stream])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
