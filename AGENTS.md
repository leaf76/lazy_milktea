# Repository Guidelines

本文件為本倉庫的貢獻者指南。協作一律使用繁體中文；程式碼註解、log 與 UI 文案使用 English。產品目標與資料模型請見 `spec/spec.md`。

## 專案結構與模組
- `src/`: React + TypeScript（入口 `src/main.tsx`；元件與樣式置於對應目錄）。
- `public/`: 靜態資產。
- `src-tauri/`: Tauri（Rust）後端與建置設定（`src/`、`Cargo.toml`、`tauri.conf.json`、`icons/`）。
- `spec/`: 產品規格與樣本資料。
- 測試：Rust 測試位於 `src-tauri`；前端建議使用 Vitest。

## 建置、測試與開發
- 安裝：`npm install`
- Web 開發：`npm run dev`（Vite 開發伺服器）
- Desktop 開發：`npm run tauri dev`
- Web 打包：`npm run build`
- Desktop 打包：`npm run tauri build`
- Rust 測試：`cd src-tauri && cargo test`
- 前端測試（建議）：在 `package.json` 加入 `"test": "vitest"`，再執行 `npm test`

## 程式風格與命名
- TypeScript：2 空格縮排、strict typing、函式/變數 `camelCase`、元件/檔名 `PascalCase.tsx`、資料夾 `kebab-case`；使用 Prettier。
- Rust：`cargo fmt`、`cargo clippy -- -D warnings`；除測試外避免 `unwrap()`。
- 日誌與註解：English；避免寫入敏感資訊。

## 測試指引
- 目標覆蓋率 ≥ 80%，優先涵蓋解析、索引、查詢與錯誤處理。
- 使用 `spec/` 樣本 bugreport 驗證事件計數、時間正規化與篩選（時間範圍 + 多條件）。
- 修復錯誤需新增對應測試；優先採 TDD（先寫失敗測試）。

## Commit 與 PR
- Commit：Conventional Commits（例：`feat(parser): add log indexing`）。
- PR：描述變更與動機、連結議題、測試方式與結果；UI 變更附截圖/GIF；必要時說明風險與回滾；CI 必須全綠。

## 安全與設定
- 禁止提交密鑰；使用環境變數或作業系統金鑰圈。限制 Tauri 指令/能力至最小權限。
- 發佈前設定嚴格 CSP（目前 `src-tauri/tauri.conf.json` 開發用 `csp: null`）。

## 架構概要
- 重載工作在 Rust：解壓 `.zip`、串流解析 `bugreport*.txt`、建置時間/倒排索引並輸出結構化 JSON；前端聚焦篩選與視覺化，巨大清單採虛擬化。
- IPC：Tauri 指令（如 `parse_bugreport(path)`、`query_logcat(filters, page, pageSize)`）。

