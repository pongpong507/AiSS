# AiSS 專案進度報告

> 最後更新：2026-04-12

## 一、目前所在 Milestone

**Milestone 0：概念驗證** — 已完成核心功能，進入優化與 Docker 化階段。

## 二、已完成的功能

### 2.1 Rust Workspace（4 個 crate + 1 個 docker 模組）

| Crate | 用途 | 狀態 |
|---|---|---|
| `shared-types` | 共用型別（Verdict, Topic, Difficulty, UserContext） | ✅ 完成 |
| `llm-gateway` | LlmProvider trait + OllamaProvider adapter | ✅ 完成 |
| `infolit-game` | 演員池 × 騙術清單 × selector × session × CLI | ✅ 完成 |
| `aiss-import` | YAML/CSV 批次匯入工具（演員 + 騙術） | ✅ 完成（不在 workspace members） |
| `docker-infolit` | 網頁版 scaffold（他人提交） | ⚠️ 有問題，見第五節 |

### 2.2 核心模組狀態

| 模組 | 檔案 | 功能 | 測試 |
|---|---|---|---|
| Actor Pool | `actor.rs` | Actor struct、YAML 載入、affinity 驗證 | 7 tests ✅ |
| Deception Catalog | `deception.rs` | DeceptionPattern struct、YAML 載入 | 3 tests ✅ |
| Selector | `selector.rs` | Affinity 加權隨機選角、15% 例外通道 | 7 tests ✅ |
| Session | `session.rs` | System prompt 組裝、對話管理、評分 | 7 tests ✅ |
| Pacing | `pacing.rs` | 回應延遲設定 | 無獨立測試 |
| OllamaProvider | `ollama.rs` | chat / chat_stream / embed / health / list_models | 無（需 Ollama） |
| Types | `types.rs` | ChatMessage / ChatRequest / ChatResponse | 5 tests ✅ |
| CLI Main | `main.rs` | --doctor / --think / --no-delay / 遊戲迴圈 | 手動測試 ✅ |

**測試合計：31 tests，全數通過。**

### 2.3 LLM Gateway 功能

- [x] `LlmProvider` trait（chat / chat_stream / embed）
- [x] `OllamaProvider`（非串流 + 串流 + embedding）
- [x] ThinkingMode 支援（`--think` 旗標）
  - 自動放大 max_tokens 4 倍（最低 2048）
  - thinking 內容存入 extensions["thinking"]
  - content 為空時 fallback 用 thinking
- [x] Health check（`/api/tags`）
- [x] 模型列表（`list_models()`）
- [x] 連線逾時 5s + 整體逾時 300s
- [x] 詳細錯誤訊息（連線失敗提示 `ollama serve`）

### 2.4 CLI 遊戲功能

- [x] `--doctor` 預檢模式（4 步驟：內容 → 連線 → 模型 → 推論）
- [x] `--think` 啟用 thinking mode
- [x] `--no-delay` 關閉 pacing 延遲
- [x] `--model` / `--ollama-url` / `--actors` / `--liars` 參數
- [x] 遊戲迴圈：開場發言 → 互動問答 → /accuse 指控 → 評分
- [x] 最大 10 回合限制
- [x] 三色評分系統（Green / Yellow / Red）

### 2.5 內容資料

- **10 位演員**：林博士(9)、王大叔(6)、李奶奶(3)、網紅Jay(4)、陳同學(5)、吳老師(7)、播報員小智(8)、黃家長(3)、小明(2)、神秘博士(11)
- **8 種騙術**：偽造引用(9)、斷章取義(6)、自信的錯誤事實(6)、假借權威(8)、恐懼訴求(3)、以偏概全(5)、過時資訊(7)、氣場壓制(11)
- **3 題種子題庫**（硬編碼在 main.rs）：海豚是魚類嗎？/ 微波爐輻射 / 大腦10%迷思

### 2.6 文件與基礎建設

- [x] README.md
- [x] ADR-001（LlmProvider trait）
- [x] ADR-002（InfoLit 遊戲迴圈）
- [x] ADR-003（Affinity 系統）
- [x] docs/000-MOC.md（Obsidian Vault 索引）
- [x] scripts/start-local.sh + start-local.ps1
- [x] .gitignore（排除 .claude/、*.pdf、target/）
- [x] Git remote: `https://github.com/pongpong507/AiSS.git`（main 分支）

## 三、未完成 / 已知問題

### 3.1 遊戲機制問題

1. **騙子永遠排第一發言** — `assemble_session()` 中 liar 永遠取 `selected[..liar_count]`，開場發言又按順序走，所以第一個發言的幾乎都是騙子
2. **固定發言順序** — 所有演員按 selected 陣列順序 1→2→3 依序發言，缺乏變化
3. **無自動繼續機制** — 玩家不輸入就會卡住，沒有 timeout 觸發 NPC 繼續對話
4. **NPC 重複率高** — 10 位演員 × 3 人/局，容易重複見到相同角色

### 3.2 語言問題

- System prompt 未明確指定使用**臺灣繁體中文用語**，LLM 傾向輸出簡體/大陸用語風格

### 3.3 Docker 化問題（他人提交）

- `docker-infolit/` 與 `server/` 有多處問題（見第五節詳細分析）

### 3.4 尚未實作

- [ ] aiss-import 不在 workspace members 中（獨立編譯）
- [ ] 題庫外部化（目前硬編碼 3 題）
- [ ] Web 版（Milestone 1）
- [ ] 教師後台
- [ ] 多人 / WebSocket

## 四、程式碼統計

| 項目 | 數值 |
|---|---|
| Rust 原始碼 | ~2,400 行 |
| 測試數量 | 31 |
| Crate 數量 | 4（workspace）+ 1（aiss-import）+ 1（docker-infolit） |
| 演員 YAML | 10 |
| 騙術 YAML | 8 |
| Git commits | 3 |

## 五、Docker 化 scaffold 問題分析

最新 commit `07717d0` 由他人提交，新增了 `docker-infolit/`、`docker/Dockerfile`、`server/src/main.rs`。以下是發現的問題：

### 5.1 編譯錯誤

1. **`docker-infolit/src/main.rs`** 第 8 行：
   ```rust
   use llm_gateway::llm_gateway::generate_content;
   ```
   - `llm_gateway` crate 沒有 `llm_gateway` 子模組，也沒有 `generate_content` 函數
   - 這行會導致編譯失敗

2. **`docker-infolit/src/main.rs`** 第 23 行：
   ```rust
   axum::Server::bind(&addr)
   ```
   - `axum 0.7` 已移除 `Server::bind()`，需改用 `tokio::net::TcpListener` + `axum::serve()`

3. **`server/src/main.rs`** — 與 `docker-infolit/src/main.rs` 幾乎相同的複本，有同樣的問題，且不在 workspace 中、沒有 Cargo.toml

### 5.2 Dockerfile 問題

1. **缺少 builder stage** — 只有 `FROM debian:bullseye-slim` 但 `COPY --from=builder` 引用了不存在的 `builder` stage
2. **缺少 multi-stage build** 的第一階段（Rust 編譯環境）
3. **缺少執行期依賴**（如 `ca-certificates`、`libssl`）
4. **監聽 127.0.0.1** — 在 Docker 容器內應監聽 `0.0.0.0` 才能從外部連線

### 5.3 架構問題

- 沒有整合 `infolit-game` 的遊戲邏輯，只有一個空的 `/api/generate` endpoint
- 沒有靜態檔案服務（缺少前端 HTML/JS）
- 沒有 WebSocket 支援（遊戲對話需要即時通訊）
- `tower-http` 引入了 `fs` feature 但未使用

## 六、硬體與測試環境

- **GPU**: NVIDIA GeForce RTX 4060 Ti 16GB
- **Ollama**: 本機 port 22549
- **測試模型**: gemma4:latest（支援 thinking mode）
- **已驗證**: `--doctor` 預檢 + `--think` 模式均通過
