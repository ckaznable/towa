# First Run Checklist

## 1. 設定檔

- 建立 `~/.config/towa/config.toml`
- 至少確認有一個 `[[llm.agents]]`
- 若不想把 key 寫進檔案，可改用 `GEMINI_API_KEY`

## 2. 啟動路徑

- SQLite 預設位置：`~/.local/share/towa/towa.db`
- 若要改位置，設定 `TOWA_DB_PATH`
- 若要改 config 位置，設定 `TOWA_CONFIG`

## 3. 啟動後端

```bash
cargo run
```

確認：

- API: `http://127.0.0.1:3000/api/health`
- 回應應為 `{"status":"ok","service":"towa-api"}`

## 4. 啟動前端

```bash
cd web
npm install
npm run dev
```

或直接：

```bash
./scripts/dev.sh
```

確認：

- Frontend: `http://127.0.0.1:5173`
- `/api` 已由 Vite proxy 到 Axum

## 5. 新增來源

- 先在前端 `Manage feeds` 新增 RSS 或 Atom feed
- 可選擇是否指派 Gemini agent
- 新來源會先做格式驗證

## 6. 驗證抓取與保留策略

- scheduler 會依 `next_fetch_at` 抓取來源
- 優先尊重 `Cache-Control` / `Expires`
- 一般文章預設保留 30 天
- 收藏文章永久保留

## 7. 驗證 LLM worker

若有設定 `GEMINI_API_KEY`：

- 新文章且來源有指派 agent 時，狀態會進入 `pending`
- 後續會變成 `processing`、`done` 或 `failed`
- 成功結果顯示在 `llm_summary`
- 錯誤顯示在 `llm_error`

若沒有設定 `GEMINI_API_KEY`：

- API 與前端仍可使用
- 但 LLM worker 不會啟動

## 8. 常用檢查

```bash
cargo test
cd web && npm run build
```
