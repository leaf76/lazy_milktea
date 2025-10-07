# Repository Guidelines

This guide applies to the entire repository. Collaborate in Traditional Chinese; keep code comments, logs, and UI copy in English. For product goals and data models, see `spec/spec.md`.

## Project Structure
- `src/`: React + TypeScript app (entry `main.tsx`; components and styles live here).
- `public/`: Static assets.
- `src-tauri/`: Tauri (Rust) backend and build config (`src/`, `Cargo.toml`, `tauri.conf.json`, `icons/`).
- `spec/`: Product spec and architecture (`spec.md`, sample data).

## Architecture Overview (per spec)
- Heavy work in Rust: unzip `.zip`, stream-parse large `bugreport*.txt`, and build time/inverted indexes. The frontend focuses on filtering and visualization.
- Emit structured JSON (e.g., device, events, tombstones, log indexes) for incremental loading in the UI.
- IPC via Tauri commands (e.g., `parse_bugreport(path)`, `query_logcat(filters, page, pageSize)`).
- For huge inputs, avoid full memory loads: lazy extract, chunked scanning, and virtualized lists.

## Dev, Build, and Test
- Install: `npm install`
- Web dev: `npm run dev`
- Desktop dev: `npm run tauri dev`
- Frontend build: `npm run build`
- Desktop bundle: `npm run tauri build`
- Rust tests: `cd src-tauri && cargo test`
- Frontend tests (recommended): Vitest; add script `"test": "vitest"`.

## Coding Style & Naming
- TypeScript: 2-space indent; strict typing; functions/vars `camelCase`; components/files `PascalCase.tsx`; dirs `kebab-case`; format with Prettier.
- Rust: `cargo fmt`, `cargo clippy -- -D warnings`; avoid `unwrap()` outside tests.

## Testing Guidelines
- Unit/integration tests cover core paths: parsing, indexing, querying, and error handling; target coverage ≥ 80%.
- Validate using sample bugreports in `spec/`: event counts, timestamp normalization, and log filtering (time range + multi-criteria).
- TDD: write a failing test first; PRs must include tests and results.

## Commit & PR Guidelines
- Conventional Commits (e.g., `feat(parser): add log indexing`).
- PRs must include: summary with linked issues, how-to-test and results, screenshots/GIFs for UI changes, and risks/rollback when relevant.

## Security & Configuration
- Do not commit secrets; use env vars or the OS keychain. Restrict Tauri commands/capabilities to least privilege.
- Before release, set a strict CSP (current `tauri.conf.json` has `csp: null` for development only).
