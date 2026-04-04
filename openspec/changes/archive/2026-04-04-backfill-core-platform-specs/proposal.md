## Why

目前產品已經實作 RSS/Atom 來源管理、排程抓取、SQLite 保留策略、Gemini batch 後處理與 Web UI，但主 `openspec/specs/` 只有 `article-favorites`。這造成實作範圍與正式規格不一致，後續維護、驗證與擴充都缺少穩定契約。

## What Changes

- 將目前已實作的 feed source 管理能力補回主 spec。
- 將 scheduler 的條件式抓取、freshness header 尊重與 fallback 抓取節奏補回主 spec。
- 將 SQLite 文章保存、30 天 retention 與收藏保留語意補回主 spec。
- 將 Gemini batch LLM 後處理、狀態流轉、重試與 admin retry 能力補回主 spec。
- 將 reader API 與現有 Web UI 對接能力補回主 spec。

## Capabilities

### New Capabilities
- `feed-source-management`: 定義 RSS/Atom 來源的建立、更新、刪除、啟用狀態與 agent 指派能力。
- `feed-fetch-scheduling`: 定義來源排程抓取、ETag/Last-Modified 條件式請求、freshness header 尊重與 fallback 間隔能力。
- `article-storage-retention`: 定義文章 SQLite 持久化、去重、30 天保留與收藏例外保留能力。
- `llm-agent-post-processing`: 定義 Gemini batch 後處理、處理狀態、批次輪詢、失敗重試與 admin retry 能力。
- `reader-api-and-web-ui`: 定義 reader API、前端對接資料契約、SPA fallback 與主要閱讀/設定流程能力。

### Modified Capabilities
- 無。

## Impact

- 影響 `openspec/specs/` 主規格樹，新增 5 個核心 capability specs。
- 影響後續 OpenSpec 變更流程，讓既有 `src/app.rs`、`src/scheduler.rs`、`src/db.rs`、`src/llm.rs`、`web/` 對應到正式規格。
- 不引入新的執行期依賴，不改變現有 API 或產品行為，主要是補齊正式規格與任務追蹤。
