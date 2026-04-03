## Why

目前系統只有一般文章保留與 30 天清理機制，缺少明確的「收藏」能力定義。使用者需要能把有價值的文章標記為收藏，讓它們脫離一般保留週期，並能被後端穩定查詢、管理與回傳。

## What Changes

- 新增文章收藏能力，讓文章可被明確標記為收藏狀態，並作為獨立資料語意而非一般歸檔。
- 新增收藏文章的查詢與管理行為，讓後端能穩定回傳收藏清單與單篇收藏狀態。
- 明確定義收藏文章不受一般 30 天清理策略影響，直到使用者取消收藏為止。
- 補齊收藏相關後端 API、資料持久化規則、觀測與測試覆蓋。

## Capabilities

### New Capabilities
- `article-favorites`: 定義文章收藏、取消收藏、收藏查詢與收藏保留語意。

### Modified Capabilities
- 無。

## Impact

- 影響 `src/app.rs`、`src/state.rs`、`src/db.rs`、`src/domain.rs` 的後端 API 與資料模型。
- 影響 SQLite schema 與 migration，需明確保存收藏狀態。
- 影響 retention cleanup 邏輯與測試案例。
- 影響後續前端與其他 client 對接時的收藏相關 API 合約。
