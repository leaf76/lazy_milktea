# Repository Guidelines

本文件為本倉庫的貢獻者指南。協作一律使用繁體中文；程式碼註解、log 與 UI 文案使用 English。產品目標與資料模型請見 `spec/spec.md`。

## 專案摘要
- 目標：解析 Android bugreport（zip/txt），提供 Dashboard、Logcat 檢索、事件摘要等視圖。
- 架構：重解析與索引在 Rust（Tauri backend），前端僅負責篩選與呈現。
- IPC：Tauri Commands（例：`parse_bugreport`、`query_logcat_v2`、`get_logcat_stats`）。

## 專案結構與模組
- `src/`: React + TypeScript（入口 `src/main.tsx`；元件與樣式置於對應目錄）。
- `src/components/`: UI 與功能組件（Logcat 相關在 `components/logcat/`）。
- `public/`: 靜態資產。
- `src-tauri/`: Tauri（Rust）後端與建置設定（`src/`、`Cargo.toml`、`tauri.conf.json`、`icons/`）。
- `src-tauri/src/parser/`: Bugreport 解析模組（logcat/device/entrypoint）。
- `src-tauri/src/index/`: 索引與 SQLite cache（builder/streaming/sqlite）。
- `src-tauri/src/query/`: 查詢與分頁（cursor/executor/filter）。
- `spec/`: 產品規格與樣本資料。

## Build / Lint / Test 指令
### 前端（Web）
- 安裝依賴：`npm install`
- 開發模式：`npm run dev`
- 打包：`npm run build`
- 預覽：`npm run preview`
- 型別檢查（等同 build 的 tsc）：`npm run build`
- 單一測試：目前未設定前端測試指令；若需新增，建議採 Vitest（在 `package.json` 新增 `"test": "vitest"`）。

### Tauri Desktop
- Desktop 開發：`npm run tauri dev`
- Desktop 打包：`npm run tauri build`

### Rust（Tauri backend）
- 進入目錄：`cd src-tauri`
- 全部測試：`cargo test`
- 單一測試（函式）：`cargo test test_name`
- 單一測試（模組路徑）：`cargo test module::submodule::test_name`
- 格式化：`cargo fmt`
- Lint：`cargo clippy -- -D warnings`

## 程式風格與命名（通用）
- 語言：TypeScript + Rust；嚴格型別，不可使用 `any` 隨意繞過。
- 缩排：TypeScript 2 空格；Rust 4 空格（由 rustfmt 管理）。
- 字串：TypeScript 以雙引號為主，與現有風格一致。
- 檔名：React 元件 `PascalCase.tsx`，資料夾 `kebab-case`。
- 命名：函式/變數 `camelCase`；Rust 使用 `snake_case`。
- 註解/Log/UI 文案：使用 English，避免敏感資訊。

## TypeScript / React 指南
- Hooks：遵守 React Hooks 規則，不在條件式中呼叫 hooks。
- Props/State：為所有 state 與 props 定義明確型別，避免隱式 any。
- Tauri 呼叫：集中在 `invoke` 層，回傳型別使用泛型標註（例：`invoke<ParseSummary>`）。
- Error handling：使用 `try/catch`，錯誤訊息顯示需安全且可理解；避免空 catch。
- Side effects：使用 `useEffect` 時確保清理 handler（`listen` 必須回收）。
- 清單效能：大量 log 需使用虛擬清單（已使用 `react-virtuoso`）。

## Rust 指南
- 錯誤處理：使用 `Result<T, LogcatError>`；避免 `unwrap()`（測試例外）。
- 自訂錯誤：集中在 `src-tauri/src/error.rs`，新增錯誤型別時要對應 `Display`。
- IO/解析：優先使用 streaming，避免一次讀入大檔。
- 併行：解析/索引可使用 `rayon`；避免共享可變狀態。
- SQLite：使用 `rusqlite`，保持 statement 參數化，不串接 SQL 字串。

## Import 與模組規範
- TypeScript：順序建議 `react` → third-party → internal → styles。
- Rust：`std` → external crates → internal modules。
- 僅在需要時引入；未使用的 import 必須移除（tsconfig 已啟用 noUnused）。

## 錯誤處理與記錄
- Rust：錯誤需要分類且可追蹤；在必要時使用 `thiserror` 與 `anyhow`。
- 前端：顯示人類可讀錯誤，不外洩系統內部路徑或敏感資訊。
- Log：避免敏感資訊；如需記錄，使用結構化資訊與 trace id（與專案標準一致）。

## 測試指引
- 覆蓋率目標 ≥ 80%，優先涵蓋解析、索引、查詢與錯誤處理。
- 使用 `spec/` 樣本 bugreport 驗證事件計數、時間正規化與篩選（時間範圍 + 多條件）。
- 修復錯誤需新增對應測試；優先採 TDD（先寫失敗測試）。
- 單元測試命名（Rust）：`test_<feature>_<scenario>`。

## Commit 與 PR
- Commit：Conventional Commits（例：`feat(parser): add log indexing`）。
- PR：描述變更與動機、連結議題、測試方式與結果；UI 變更附截圖/GIF；必要時說明風險與回滾；CI 必須全綠。

## 安全與設定
- 禁止提交密鑰；使用環境變數或作業系統金鑰圈。
- 限制 Tauri 指令/能力至最小權限。
- 發佈前設定嚴格 CSP（目前 `src-tauri/tauri.conf.json` 開發用 `csp: null`）。

## Cursor / Copilot 規則
- 未找到 `.cursorrules`、`.cursor/rules/` 或 `.github/copilot-instructions.md`。

## 其他注意事項
- 避免大檔案一次載入；前端採分頁/分塊結果渲染。
- 不要刪除或回復無關變更；若需清理請先確認。
- 若需要修改產品規格，更新 `spec/spec.md` 並說明動機。

