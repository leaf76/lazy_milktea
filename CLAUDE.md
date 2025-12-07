# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lazy Milktea is a Tauri desktop app for analyzing Android bugreport files. It parses `bugreport-*.zip` or `.txt` files and provides an interactive UI for viewing device info, logcat entries, ANR/crash events, and system metrics.

## Build & Development Commands

```bash
# Install dependencies
npm install

# Development
npm run dev          # Web dev server (Vite)
npm run tauri dev    # Desktop app with hot reload

# Production builds
npm run build        # Web build (tsc + vite)
npm run tauri build  # Desktop app bundle

# Rust backend
cd src-tauri
cargo test           # Run Rust tests
cargo fmt            # Format Rust code
cargo clippy -- -D warnings  # Lint with strict warnings
```

## Architecture

### IPC Boundary

Heavy parsing happens in Rust; frontend only handles filtering and rendering:

- `parse_bugreport(path)` → extracts device info, counts ANR/crashes, builds logcat index
- `query_logcat(filters, page, pageSize)` → paginated logcat query
- `query_logcat_stream(filters, cursor, limit)` → cursor-based streaming for virtual lists

### Backend (src-tauri/src/)

- **lib.rs**: Tauri commands and app state management (holds `last_cache_dir` for parsed report)
- **parser.rs**: Core parsing logic
  - `parse_entrypoint()` dispatches zip vs txt parsing
  - `build_logcat_index()` creates `logcat.jsonl` plus time/inverted indexes in `~/.lazy_milktea_cache/<report>/`
  - `stream_logcat()` uses seek + time index for efficient filtered queries
- **types.rs**: Shared data structures (`DeviceInfo`, `LogRow`, `LogFilters`)

### Frontend (src/)

- **types.ts**: TypeScript types mirroring Rust structs (camelCase)
- **components/LogcatView.tsx**: Virtual list rendering with react-virtuoso
- **components/AppShell.tsx**: App layout and navigation

### Cache Structure

Parsed data is cached in `~/.lazy_milktea_cache/<report-name>/`:
- `logcat.jsonl` - one JSON log row per line
- `log_summary.json` - total counts, E/F stats, time range
- `log_time_index.json` - minute-based byte offsets for seek
- `log_inv_tag.json`, `log_inv_pid.json` - inverted indexes (sampled)

## Key Patterns

### Logcat Parsing

The logcat regex pattern (threadtime format):
```
^(\d{2}-\d{2})\s+(\d{2}:\d{2}:\d{2}\.\d{3})\s+(\d+)\s+(\d+)\s+([VDIWEF])\s+([^:]+):\s(.*)$
```

Time normalization uses `persist.sys.timezone` from the bugreport to convert local timestamps to UTC ISO format.

### Filter Types

`LogFilters` supports: `tsFrom/tsTo` (time range), `levels` (V/D/I/W/E/F), `tag`, `pid`, `tid`, `text`, `notText`, `textMode` (plain/regex), `caseSensitive`

## Code Style

- **TypeScript**: 2-space indent, strict typing, Prettier
- **Rust**: `cargo fmt`, avoid `unwrap()` outside tests, use `anyhow::Result`
- **Naming**: camelCase (TS), snake_case (Rust), PascalCase for components/types
- **Serde**: All shared types use `#[serde(rename_all = "camelCase")]`

## Testing

Run Rust tests with sample snippets:
```bash
cd src-tauri && cargo test
```

Tests in `parser.rs` validate:
- Device info extraction from bugreport snippets
- Logcat indexing and filtered queries
- TID, exclude text, and regex filter combinations
