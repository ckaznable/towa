# Backend API Contract

這份文件定義目前可供前端先行對接的 Axum API 介面。

## Current Scope

- 後端使用 SQLite 持久化，啟動時會自動建立資料庫並套用 migration。
- source 建立/更新時會即時抓取遠端 feed URL，並用 `feed-rs` 驗證是否為 RSS 或 Atom。
- agent 清單與 Gemini batch 設定由 XDG `config.toml` 載入；若設定檔不存在，會使用內建 Gemini 預設值。
- 背景 scheduler 會自動啟動，依 `next_fetch_at` 挑選來源抓取，支援 `ETag` / `Last-Modified` 條件式請求。
- 若來源提供 `Cache-Control` 或 `Expires`，scheduler 會優先採用；否則使用內建 fallback 區間。
- scheduler 會對文章使用穩定 dedupe key，避免同一來源重複寫入相同文章。
- 每輪背景工作都會先執行 retention cleanup：刪除超過 30 天且未收藏的文章，收藏文章不會被刪除。
- 若來源有指派 agent，新文章會進入後處理工作佇列；worker 預設走 Gemini Batch API，並將每篇文章狀態、輸出與錯誤落地到 SQLite。
- 若未設定 `GEMINI_API_KEY`，HTTP API 仍可啟動，但 LLM worker 會停用。

## Runtime Paths

- Config: `~/.config/towa/config.toml`
- Database: `~/.local/share/towa/towa.db`

可用環境變數覆蓋：

- `TOWA_CONFIG`
- `TOWA_DB_PATH`
- `GEMINI_API_KEY`
- `LOG_FORMAT=json|text`

## Config Example

```toml
[llm]
api_key = "optional-inline-key"
batch_poll_interval_seconds = 30
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

說明：

- `api_key` 可省略，若有設定 `GEMINI_API_KEY` 環境變數會優先使用。
- `batch_enabled` 目前保留為 agent 能力欄位；後端預設仍走 Gemini Batch API。

## Base URL

```text
http://127.0.0.1:3000
```

## Endpoints

### Health

`GET /api/health`

```json
{
  "status": "ok",
  "service": "towa-api"
}
```

### Agents

`GET /api/agents`

```json
{
  "items": [
    {
      "id": "gemini-brief",
      "label": "Gemini Brief",
      "provider": "gemini",
      "model": "gemini-2.5-flash",
      "batch_enabled": true
    }
  ]
}
```

### Sources

`GET /api/sources`

```json
{
  "items": [
    {
      "id": "6a6f1d6f-fb0e-482f-bd66-42de6678878f",
      "title": "Rust Blog",
      "feed_url": "https://example.com/feed.xml",
      "feed_kind": "rss",
      "enabled": true,
      "assigned_agent_id": "gemini-brief",
      "validation_status": "validated",
      "last_fetch_at": null,
      "next_fetch_at": null,
      "created_at": "2026-03-31T15:00:00Z",
      "updated_at": "2026-03-31T15:00:00Z"
    }
  ]
}
```

`POST /api/sources`

```json
{
  "title": "Rust Blog",
  "feed_url": "https://example.com/feed.xml",
  "enabled": true,
  "assigned_agent_id": "gemini-brief"
}
```

`GET /api/sources/{id}`

`PATCH /api/sources/{id}`

```json
{
  "title": "Rust Blog Updated",
  "feed_url": "https://example.com/updated.xml",
  "enabled": false
}
```

`PUT /api/sources/{id}/agent`

```json
{
  "assigned_agent_id": "gemini-deep-tech"
}
```

若要清除指派：

```json
{
  "assigned_agent_id": null
}
```

`DELETE /api/sources/{id}`

回傳 `204 No Content`

### Articles

`GET /api/articles`

支援 query:

- `source_id=<uuid>`
- `bookmarked=true|false`

```json
{
  "items": [
    {
      "id": "41ca1a10-d274-4fcb-b5a2-d9fcbf507ccc",
      "source_id": "6a6f1d6f-fb0e-482f-bd66-42de6678878f",
      "source_title": "Rust Blog",
      "title": "Tokio 2 planning notes",
      "summary": "A preview of async runtime changes.",
      "url": "https://example.com/articles/tokio-2",
      "published_at": "2026-03-31T15:10:00Z",
      "fetched_at": "2026-03-31T15:12:00Z",
      "bookmarked": false,
      "llm_status": "pending"
    }
  ]
}
```

`GET /api/articles/{id}`

```json
{
  "id": "41ca1a10-d274-4fcb-b5a2-d9fcbf507ccc",
  "source_id": "6a6f1d6f-fb0e-482f-bd66-42de6678878f",
  "source_title": "Rust Blog",
  "title": "Tokio 2 planning notes",
  "summary": "A preview of async runtime changes.",
  "url": "https://example.com/articles/tokio-2",
  "published_at": "2026-03-31T15:10:00Z",
  "fetched_at": "2026-03-31T15:12:00Z",
  "bookmarked": false,
  "llm_status": "failed",
  "llm_summary": null,
  "llm_error": "quota exceeded"
}
```

LLM 狀態語意：

- `pending`: 已入佇列，等待送出 batch。
- `processing`: 已送出 Gemini Batch API，等待結果。
- `done`: 已成功寫入 `llm_summary`。
- `failed`: 已達重試上限，錯誤保留在 `llm_error`。

### Bookmarks

`PUT /api/articles/{id}/bookmark`

```json
{
  "bookmarked": true
}
```

`GET /api/bookmarks`

等同於 `GET /api/articles?bookmarked=true`

### Admin Processing

`GET /api/admin/processing`

用來查看目前 queue、active batches、failed jobs。

```json
{
  "pending_jobs": [
    {
      "article_id": "41ca1a10-d274-4fcb-b5a2-d9fcbf507ccc",
      "agent_id": "gemini-brief",
      "source_title": "Rust Blog",
      "title": "Tokio 2 planning notes",
      "published_at": "2026-03-31T15:10:00Z"
    }
  ],
  "active_batches": [
    {
      "batch_name": "operations/abc123",
      "agent_id": "gemini-brief",
      "article_count": 8,
      "updated_at": "2026-04-03T08:12:00Z"
    }
  ],
  "failed_jobs": [
    {
      "article_id": "41ca1a10-d274-4fcb-b5a2-d9fcbf507ccc",
      "agent_id": "gemini-brief",
      "source_title": "Rust Blog",
      "title": "Tokio 2 planning notes",
      "attempts": 3,
      "last_error": "quota exceeded",
      "last_batch_name": "operations/abc123",
      "updated_at": "2026-04-03T08:14:00Z"
    }
  ]
}
```

`POST /api/admin/articles/{id}/retry`

將單篇文章重新排回 `pending` 狀態。

```json
{
  "retried": 1
}
```

`POST /api/admin/batches/retry`

```json
{
  "batch_name": "operations/abc123"
}
```

依 `last_batch_name` 將該 batch 相關文章重新排回 `pending` 狀態。

```json
{
  "retried": 8
}
```

## Error Format

所有錯誤都會回傳：

```json
{
  "error": "human readable message"
}
```

常見狀態碼：

- `404 Not Found`
- `422 Unprocessable Entity`
