# Stitch RSS Reference Screens

這份文件整理既有 Google Stitch 專案中的 RSS 參考畫面，供後續前端實作對照使用。

## Stitch Project

- Title: `RSS`
- Project ID: `14281094935872371486`
- Device: `DESKTOP`
- Theme: `DARK`

## Recommended Reference Screens

### 1. Dashboard

- Title: `Wide RSS Dashboard (Single User)`
- Screen ID: `00851c7a31364d6290bef35e9ed38d0c`
- 用途: 對應首頁閱讀器主畫面，適合拆成左側來源欄、中央文章列表、右側文章預覽。

### 2. Alternate Dashboard

- Title: `RSS Dashboard (No Social, No Add Feed)`
- Screen ID: `2c44ecbcbc094aaea666101d0b762ff9`
- 用途: 較純閱讀導向的 dashboard 版本，可作為簡化版資訊密度與 dark mode 版型參考。

### 3. Article Reader

- Title: `Article Reader (Pure Focus, with Link)`
- Screen ID: `5fdb498d0da94053a5643dd369702c98`
- 用途: 對應文章詳情頁，適合映射到內文閱讀、原文連結、收藏與 LLM 後處理結果區塊。

### 4. Feed Intelligence

- Title: `Feed Intelligence (Single User)`
- Screen ID: `bb83b1eab0e542679a053f8e707c179c`
- 用途: 可作為文章摘要、AI 洞察、來源聚合分析與處理狀態展示的參考。

### 5. Settings

- Title: `Global Settings (Single User)`
- Screen ID: `4d2959d24a70468f9170f7c1a04e3966`
- 用途: 可延伸為來源設定、抓取頻率邊界、全域保留策略與 Gemini provider 設定頁。

### 6. Agent Management

- Title: `Agent Management (Single User)`
- Screen ID: `98894ea2af8945968efb8d013e90b796`
- 用途: 對應來源與 agent 指派、agent profile 管理、處理策略切換。

## Suggested Mapping To OpenSpec Tasks

- `6.1 建立 Dark Mode 預設主題與基礎版面`
  - 主要參考: `Wide RSS Dashboard (Single User)`, `RSS Dashboard (No Social, No Add Feed)`
- `6.2 實作來源管理介面（新增/編輯/停用/刪除/agent 指派）`
  - 主要參考: `Global Settings (Single User)`, `Agent Management (Single User)`
- `6.3 實作文章清單與詳情閱讀介面，顯示 LLM 處理狀態與結果`
  - 主要參考: `Wide RSS Dashboard (Single User)`, `Article Reader (Pure Focus, with Link)`, `Feed Intelligence (Single User)`
- `6.4 實作收藏操作與收藏檢視頁，確保可存取永久收藏文章`
  - 主要參考: `Article Reader (Pure Focus, with Link)`，後續需再補 bookmark archive 專用畫面

## Notes

- 目前 Stitch 專案內沒有明確命名為 `Source Management` 或 `Bookmarks` 的獨立畫面。
- 後續若要補齊來源管理與收藏頁，可直接在同一個 Stitch project 上增補 screen，而不是重建新專案。
