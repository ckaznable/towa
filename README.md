# Towa RSS Reader

Towa 是一個單機自架的 RSS / Atom reader，包含：

- Axum 後端 API 與背景 worker
- Vite 8 + Vue 前端
- SQLite 文章儲存
- 來源排程抓取、HTTP cache-aware freshness 判斷
- Gemini Batch API 文章後處理
- 永久收藏與 30 天保留策略

## Requirements

- Rust toolchain
- Node.js 25+
- npm 11+

## Project Layout

- `src/`: Axum API、scheduler、LLM worker、SQLite access
- `migrations/`: SQLite schema migrations
- `web/`: Vite 8 + Vue frontend
- `scripts/dev.sh`: 本地同時啟動後端與前端的開發腳本
- `openspec/changes/build-rss-reader-platform/`: OpenSpec artifacts

## OpenSpec Coverage

目前主 `openspec/specs/` 已涵蓋這些核心能力：

- `feed-source-management`
- `feed-fetch-scheduling`
- `article-storage-retention`
- `llm-agent-post-processing`
- `reader-api-and-web-ui`
- `article-favorites`

## Runtime Paths

預設使用 XDG 目錄：

- Config: `~/.config/towa/config.toml`
- Database: `~/.local/share/towa/towa.db`

可用環境變數覆蓋：

- `TOWA_CONFIG`
- `TOWA_DB_PATH`
- `GEMINI_API_KEY`
- `PORT`

## Config Example

```toml
[llm]
api_key = "optional-inline-key"
batch_poll_interval_seconds = 300
batch_submit_size = 16
retry_limit = 3

[[llm.agents]]
id = "gemini-brief"
label = "Gemini Brief"
provider = "gemini"
model = "gemini-2.5-flash"
system_prompt = "Write a concise 3-5 sentence summary for a reader."
batch_enabled = true

[[llm.agents]]
id = "gemini-deep-tech"
label = "Gemini Deep Tech"
provider = "gemini"
model = "gemini-2.5-flash"
system_prompt = "Explain technical points, risks, and practical next actions."
batch_enabled = true
```

## Development

安裝前端依賴：

```bash
cd web
npm install
```

只跑後端：

```bash
cargo run
```

只跑前端：

```bash
cd web
npm run dev
```

一起跑：

```bash
./scripts/dev.sh
```

開發時：

- Axum API 預設在 `http://127.0.0.1:3000`
- Vite dev server 預設在 `http://127.0.0.1:5173`
- `web/vite.config.ts` 已把 `/api` proxy 到 Axum

## Build

後端：

```bash
cargo build --release
```

前端：

```bash
cd web
npm run build
```

## Test

後端測試：

```bash
cargo test
```

前端型別檢查與 production build：

```bash
cd web
npm run build
```

## Scheduler Behavior

每個來源會保存：

- `ETag`
- `Last-Modified`
- `last_fetch_at`
- `next_fetch_at`

下一次抓取時間計算順序：

1. `Cache-Control: s-maxage` / `max-age`
2. `Expires`
3. fallback interval

這讓 scheduler 優先尊重來源的 cache / expiry header，而不是固定頻率暴力抓取。

fallback interval 目前邊界是：

- 最短 `1` 小時
- 最長 `6` 小時
- fetch 失敗時以最短間隔重試

## Article Retention

- 一般文章：預設保留 30 天
- 收藏文章：永久保留

背景 scheduler 每輪會先執行 retention cleanup，再抓取到期來源。

## Gemini Post-Processing

- 來源可指派單一 agent
- 新文章若有 agent 指派，會進入 `pending`
- worker 預設走 Gemini Batch API
- 狀態模型：`pending` / `processing` / `done` / `failed`
- 成功結果寫入 `llm_summary`
- 錯誤寫入 `llm_error`
- 依 `retry_limit` 自動重試

若未設定 `GEMINI_API_KEY`，HTTP API 與前端仍可使用，但 LLM worker 不會啟動。

## Frontend Scope

目前前端已包含：

- dark mode dashboard
- 全來源 stream 與 favorites 視圖
- 來源管理：新增、編輯、啟用/停用、刪除、agent 指派
- 文章清單與文章詳情閱讀
- LLM 狀態、摘要、錯誤顯示
- 收藏切換與永久收藏檢視

前端文章流目前只顯示 `llm_status = done` 的項目；若文章需經過 LLM，排序與顯示時間以 `available_at` 為準，也就是 batch 完成回寫時間，而不是原始抓取時間。

## Favorites API

後端已把收藏提升成正式能力，前端與其他 client 應優先使用：

- `GET /api/favorites`
- `PUT /api/articles/{id}/favorite`
- `GET /api/articles?favorited=true`

相容性考量下，舊的 bookmark alias 仍保留：

- `GET /api/bookmarks`
- `PUT /api/articles/{id}/bookmark`
- `GET /api/articles?bookmarked=true`

response 目前同時回傳：

- `favorited`: 正式收藏語意
- `bookmarked`: 舊欄位 alias

## Deployment Notes

單機部署最簡單：

1. 在 `web/` 執行 `npm run build`
2. 啟動 Axum binary
3. Axum 會直接提供 `web/dist`，且所有非 `/api` 路徑都會 fallback 到前端 `index.html`
4. 提供 `~/.config/towa/config.toml` 與 `GEMINI_API_KEY`

如果只想先驗證抓取與 API，可不提供 `GEMINI_API_KEY`。
