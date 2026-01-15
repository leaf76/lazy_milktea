# Lazy Milktea

Android bugreport 解析工具，提供 Dashboard、Logcat 檢索、事件摘要等視圖。

## 概述

本專案為 Tauri 桌面應用程式，支援解析 Android bugreport（zip/txt）。
重解析與索引由 Rust（Tauri backend）處理，前端僅負責篩選與呈現。

IPC 透過 Tauri Commands 進行：`parse_bugreport`、`query_logcat_v2`、`get_logcat_stats`。

## 功能

- 解析 Android bugreport（zip / txt）
- Dashboard 總覽
- Logcat 檢索與多條件篩選
- 事件摘要視圖
- 大量日誌虛擬清單渲染

## 技術棧

- Frontend: React + TypeScript + Vite
- Backend: Rust + Tauri
- Index/Cache: SQLite

## 專案結構

```
lazy_milktea/
├── src/                      # React + TypeScript 前端
│   ├── main.tsx              # 入口
│   └── components/           # UI 與功能元件
│       └── logcat/           # Logcat 相關元件
├── public/                   # 靜態資產
├── src-tauri/                # Tauri Rust 後端
│   ├── src/
│   │   ├── parser/           # Bugreport 解析（logcat/device/entrypoint）
│   │   ├── index/            # 索引與 SQLite cache
│   │   ├── query/            # 查詢與分頁（cursor/executor/filter）
│   │   └── error.rs          # 錯誤定義
│   ├── Cargo.toml
│   └── tauri.conf.json
└── spec/                     # 產品規格與樣本資料
```

## 環境設定

### 前置需求

- Node.js（建議 LTS）
- Rust（stable）
- Tauri 開發環境（請參考 https://tauri.app/start/prerequisites/）

### 安裝

```bash
npm install
```

## 開發指令

### 前端（Web）

```bash
npm run dev
npm run build
npm run preview
```

> 前端測試目前未設定；如需新增，建議採用 Vitest。

### Tauri Desktop

```bash
npm run tauri dev
npm run tauri build
```

### Rust（Backend）

```bash
cd src-tauri
cargo test
cargo test test_name
cargo test module::submodule::test_name
cargo fmt
cargo clippy -- -D warnings
```

## 規格文件

產品目標與資料模型請參考 [`spec/spec.md`](./spec/spec.md)。

## 開發注意事項

- 協作文件使用繁體中文；程式碼註解、Log 與 UI 文案使用 English
- TypeScript 嚴格型別，避免使用 `any`
- Rust 錯誤處理使用 `Result<T, E>`，避免 `unwrap()`
- Commit 格式為 Conventional Commits（例：`feat(parser): add log indexing`）
- 詳細規範請參考 [`AGENTS.md`](./AGENTS.md)

## IDE 建議

- [VS Code](https://code.visualstudio.com/)
- [Tauri Extension](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
