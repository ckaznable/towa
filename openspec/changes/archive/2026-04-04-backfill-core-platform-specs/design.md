## Context

目前程式碼已經提供完整的 RSS reader 平台能力：Axum API、SQLite 儲存、背景 scheduler、Gemini batch 後處理、Vue Web UI 與 SPA fallback。但 `openspec/specs/` 主規格目前只有 `article-favorites`，其餘核心能力只存在於實作、README 與 API 文件中。這讓後續任何功能調整都缺少正式規格基準，也使驗證實作是否偏離既有行為變得困難。

這次變更的目標不是新增產品能力，而是把已存在、已上線的能力回填成正式 spec。設計上的重點是忠實反映目前行為，避免在規格回填時偷偷改掉產品契約。

## Goals / Non-Goals

**Goals:**
- 將既有 source 管理、scheduler、retention、LLM 後處理與 reader API/UI 能力補回主 spec。
- 讓目前 API、資料模型與背景 worker 行為有對應的正式需求敘述。
- 讓未來變更可以基於主 spec 做 diff，而不是直接以程式碼互相比對。
- 保持與目前 `article-favorites` 主 spec 相容。

**Non-Goals:**
- 不新增新的 API、資料表、背景工作或前端頁面。
- 不重新設計 scheduler 間隔、LLM provider 抽象或前端資訊架構。
- 不在這次變更中清理所有文件命名或內部 alias 歷史，例如 `bookmark`/`favorite` 的雙命名仍保留現況。

## Decisions

### 1. 以 capability 為單位補回主 spec，而不是做一份總表文件
- 決策：拆成 `feed-source-management`、`feed-fetch-scheduling`、`article-storage-retention`、`llm-agent-post-processing`、`reader-api-and-web-ui` 五個 capabilities。
- 原因：這與現有模組邊界接近，也符合 OpenSpec 按能力做 delta 的工作模式。
- 替代方案：做一份大型 `rss-reader-platform` spec。
  - 未採用原因：後續任何小變更都會落在同一份大 spec，diff 可讀性差。

### 2. 規格只回填目前已存在的穩定行為
- 決策：spec 內容以現有程式與 API 文件為準，例如 scheduler 每 30 秒檢查到期來源、fallback 抓取間隔為 15 分鐘到 6 小時、LLM worker 使用 Gemini batch、admin API 提供 processing overview 與 retry。
- 原因：這次是規格回填，不是需求擴張。
- 替代方案：順便把未來可能要做的能力也寫進 spec。
  - 未採用原因：會把尚未承諾的行為誤寫成正式契約。

### 3. Web UI spec 以對接能力為主，而不是視覺設計為主
- 決策：`reader-api-and-web-ui` 聚焦在 dashboard、settings、article reader、favorites 視圖與 SPA fallback 等行為契約，不規範具體視覺風格。
- 原因：目前 UI 正在調整，視覺不是穩定契約；穩定的是資料流與頁面責任分工。
- 替代方案：把現有 Vue 畫面細節寫成硬性 spec。
  - 未採用原因：會讓未來 UI 改版被規格綁死。

### 4. 既有 `article-favorites` spec 視為 retention 的補充，而不是重寫
- 決策：新補的 `article-storage-retention` 只描述「一般文章 30 天清理、收藏文章例外保留」的整體資料生命週期，不覆蓋 `article-favorites` 對收藏操作本身的正式契約。
- 原因：避免 capability 重疊與互相覆寫。
- 替代方案：把 favorites 全部合併進 retention spec。
  - 未採用原因：會破壞目前已存在的 capability 邊界。

## Risks / Trade-offs

- [規格回填可能遺漏邊角行為] → 以現有測試、API 文件與主要模組交叉對照，避免只看單一來源。
- [現有 alias 命名不一致] → 在 spec 中明確記錄正式語意與相容 alias，而不是假裝系統只有一種命名。
- [實作先於 spec 的歷史包袱] → 先建立能力邊界，後續若要清理命名或重構，可以再用新的 change 做差異化調整。
- [Web UI 變動較快] → spec 只固定對接與頁面責任，不固定像素級設計。

## Migration Plan

1. 新增 5 個核心 capability specs 到 change 目錄。
2. 確認 tasks 只要求回填主 spec，不要求修改執行期行為。
3. 完成後 archive，將新 specs 同步到 `openspec/specs/`。
4. 後續功能變更一律基於這批主 specs 做差異化變更。

## Open Questions

- 是否要再補一個專門描述 admin observability 的獨立 capability？目前先歸在 `llm-agent-post-processing`。
- 是否要之後再開 change，把現有 `bookmark` alias 逐步收斂成只有 `favorite`？這次先不做。
