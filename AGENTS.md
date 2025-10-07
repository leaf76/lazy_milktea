# Repository Guidelines

本文件為本倉庫協作者指南；適用於整個專案樹。協作溝通請使用繁體中文；程式碼中的 comments／logs／UI 文案請使用 English。更完整目標與資料模型請參考 `spec/spec.md`。

## 專案結構與模組
- `src/`：React + TypeScript（入口 `main.tsx`；元件與樣式置於此）。
- `public/`：靜態資源。
- `src-tauri/`：Tauri（Rust）後端與建置設定（`src/`、`Cargo.toml`、`tauri.conf.json`、`icons/`）。
- `spec/`：產品規格與架構設計（`spec.md`、樣本資料）。

## 架構概觀（依 spec）
- 重解析與索引在 Rust：解壓 `.zip`、流式解析大型 `bugreport*.txt`、建立時間/倒排索引；前端僅負責過濾與呈現。
- 輸出結構化 JSON（如 device、events、tombstones、log 索引），供前端按需載入。
- IPC 經由 Tauri commands（例如 `parse_bugreport(path)`, `query_logcat(filters, page, pageSize)`）。
- 面對超大檔：避免一次載入記憶體；採 lazy extract、分塊掃描、虛擬清單渲染。

## 開發、建置與測試
- 安裝：`npm install`
- Web 開發：`npm run dev`
- 桌面開發：`npm run tauri dev`
- 前端建置：`npm run build`
- 打包桌面：`npm run tauri build`
- Rust 測試：`cd src-tauri && cargo test`
- 前端測試（建議）：使用 Vitest，腳本可新增 `"test": "vitest"`

## 程式風格與命名
- TS：2 空白；strict；函式/變數 `camelCase`；元件/檔名 `PascalCase.tsx`；目錄 `kebab-case`；使用 Prettier。
- Rust：`cargo fmt`、`cargo clippy -- -D warnings`；避免在非測試使用 `unwrap()`。

## 測試指引
- 單元與整合測試覆蓋核心路徑：解析、索引、查詢與錯誤處理；目標覆蓋率 ≥ 80%。
- 以 `spec/` 內樣本 bugreport 驗證：事件計數、時間戳正規化、log 過濾正確性與邊界（時間區段/多條件）。
- TDD：先寫重現測試再修正；PR 必附測試與執行結果。

## Commit 與 PR 規範
- Commit 採 Conventional Commits（例：`feat(parser): add log indexing`）。
- PR 必含：摘要與關聯 issue、測試方式與結果、如涉及 UI 附截圖/GIF、必要之風險與回滾說明。

## 安全與設定
- 禁止提交祕密；使用環境變數或系統金鑰圈。限制 Tauri commands/capabilities 存取範圍。
- 發佈前設定嚴格 CSP（`tauri.conf.json` 目前 `csp: null` 僅供開發）。
