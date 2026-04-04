## 1. 主 spec 回填

- [x] 1.1 將 `feed-source-management` 主 spec 同步到 `openspec/specs/`
- [x] 1.2 將 `feed-fetch-scheduling` 主 spec 同步到 `openspec/specs/`
- [x] 1.3 將 `article-storage-retention` 主 spec 同步到 `openspec/specs/`
- [x] 1.4 將 `llm-agent-post-processing` 主 spec 同步到 `openspec/specs/`
- [x] 1.5 將 `reader-api-and-web-ui` 主 spec 同步到 `openspec/specs/`

## 2. 契約校對

- [x] 2.1 對照 `src/app.rs`、`docs/backend-api.md` 與新 specs，確認 API 契約描述一致
- [x] 2.2 對照 `src/scheduler.rs` 與新 specs，確認 scheduler、條件式抓取與 fallback 節奏描述一致
- [x] 2.3 對照 `src/db.rs`、`src/state.rs` 與 `article-favorites` 主 spec，確認 retention 與收藏例外描述一致
- [x] 2.4 對照 `src/llm.rs` 與新 specs，確認 Gemini batch、retry 與 admin retry 描述一致

## 3. 文件整理

- [x] 3.1 更新 README 或補充說明，明確指出主 spec 已涵蓋核心平台能力
- [x] 3.2 完成後 archive change，讓補回的 specs 成為正式主規格基線
