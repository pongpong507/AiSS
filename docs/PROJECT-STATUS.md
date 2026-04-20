# AiSS 專案進度報告

> 最後更新：2026-04-20

## 一、目前所在 Milestone

**Milestone 1：Web 版 MVP** — CLI 功能完成，Web 介面可玩，正進入內容擴充與遊戲平衡調整。

## 二、已完成的功能

### 2.1 Rust Workspace（5 個 crate）

| Crate | 用途 | 狀態 |
|---|---|---|
| `shared-types` | 共用型別（Verdict, Topic, Difficulty, UserContext） | ✅ 完成 |
| `llm-gateway` | LlmProvider trait + OllamaProvider adapter | ✅ 完成 |
| `infolit-game` | 演員池 × 騙術 × selector × session × CLI | ✅ 完成 |
| `infolit-web` | Axum + SSE 網頁後端（取代舊的 docker-infolit/ scaffold） | ✅ 可用 |
| `aiss-import` | YAML/CSV 批次匯入工具（獨立編譯，不在 workspace） | ✅ 完成 |

### 2.2 核心模組狀態

| 模組 | 檔案 | 功能 | 測試 |
|---|---|---|---|
| Actor Pool | `actor.rs` | Actor struct、YAML 載入、affinity 驗證 | ✅ |
| Deception Catalog | `deception.rs` | DeceptionPattern struct、YAML 載入 | ✅ |
| Topic Catalog | `topic.rs` | Topic YAML 外部載入 | ✅ |
| Selector | `selector.rs` | Affinity 加權隨機選角、15% 例外通道、騙子位置隨機 | ✅ |
| Session | `session.rs` | System prompt、eagerness 排序、18→25 字上限、多人評分 | ✅ |
| Pacing | `pacing.rs` | 回應延遲設定 | — |
| OllamaProvider | `ollama.rs` | chat / chat_stream / embed / health / list_models | 需 Ollama |
| Types | `types.rs` | ChatMessage / ChatRequest / ChatResponse | ✅ |
| CLI Main | `main.rs` | --doctor / --think / --no-delay / 遊戲迴圈 | 手動 ✅ |
| Web Main | `infolit-web/src/main.rs` | /opening /say /respond /accuse SSE endpoints | 手動 ✅ |

**測試合計：33 tests（infolit-game），全數通過。**

### 2.3 LLM Gateway 功能

- [x] `LlmProvider` trait（chat / chat_stream / embed）
- [x] `OllamaProvider`（非串流 + 串流 + embedding）
- [x] ThinkingMode 支援（`--think` 旗標，max_tokens ×4、最低 2048）
- [x] Health check（`/api/tags`）+ 模型列表
- [x] 連線逾時 5s + 整體逾時 300s
- [x] 詳細錯誤訊息（提示 `ollama serve`）

### 2.4 CLI 遊戲功能

- [x] `--doctor` 預檢（內容 → 連線 → 模型 → 推論）
- [x] `--think` / `--no-delay` / `--model` / `--ollama-url` / `--actors` / `--liars`
- [x] 遊戲迴圈：開場 → 互動 → `/accuse` → 評分，最多 10 回合
- [x] 三色評分（Green / Yellow / Red）

### 2.5 Web 遊戲功能（infolit-web）

- [x] Axum + SSE 架構，靜態前端 (`static/index.html`)
- [x] `POST /api/game/new` — 建立對局，回傳 `liar_count`
- [x] `POST /api/game/:id/opening` — 開場發言 SSE 串流
- [x] `POST /api/game/:id/say` — 玩家訊息緩衝（不阻塞 LLM）
- [x] `POST /api/game/:id/respond` — 觸發 NPC 回應 SSE 串流
- [x] `POST /api/game/:id/accuse` — 支援多人指控（`actor_indices: Vec<usize>`）
- [x] NPC 回應 25 字硬上限（prompt + max_tokens=120 + `truncate_chars` 保險）
- [x] 自動續話（auto-continue）切換

### 2.6 內容資料

- **15 位演員**：林博士、王大叔、李奶奶、網紅 Jay、陳同學、吳老師、播報員小智、黃家長、小明、神秘博士、Gamer 小豪、方圖書館員、News 主播、Nurse 林、YouTuber 美、Uncle 計程車
- **8 種騙術**：偽造引用、斷章取義、自信的錯誤事實、假借權威、恐懼訴求、以偏概全、過時資訊、氣場壓制
- **3 題題庫**（`content/topics/q-00{1,2,3}.yaml`）：外部 YAML 載入
- 4 位角色（gamer-hao、influencer-jay、xiao-ming、uncle-taxi）speech_style 加入「不說粗口」約束

### 2.7 文件與基礎建設

- [x] README.md
- [x] ADR-001（LlmProvider trait）
- [x] ADR-002（InfoLit 遊戲迴圈）
- [x] ADR-003（Affinity 系統）
- [x] docs/000-MOC.md（Obsidian Vault 索引）
- [x] docs/research/initial-plan-2026-04.md（最初完整規劃歷史參考）
- [x] scripts/start-local.sh + start-local.ps1
- [x] .gitignore（排除 .claude/、*.pdf、target/）
- [x] Git remote `https://github.com/pongpong507/AiSS.git`（main 分支，7 commits）

## 三、未完成 / 已知問題

### 3.1 遊戲機制

1. ✅ ~~騙子永遠排第一發言~~ — selector 已改隨機指派騙子位置；speaking_order 改 eagerness 加權隨機
2. ✅ ~~固定發言順序~~ — 改為 eagerness 加權，沉默演員有 `+2 silence_bonus`
3. ✅ ~~無自動繼續機制~~ — Web 端已有 auto-continue 切換
4. ⚠️ **NPC 重複率仍偏高** — 15 位演員 × 3 人/局，但同一題可能反覆見到同批角色（沒有「最近出場」去重）
5. ⚠️ **題庫只有 3 題** — 外部化完成但內容不足，玩家很快重複

### 3.2 語言

- ✅ 已指定臺灣繁體中文用語（System prompt 明列「視頻→影片、質量→品質」等對照表）

### 3.3 尚未實作

- [ ] aiss-import 整合進 workspace
- [ ] 題庫擴充（目標 ≥ 20 題，涵蓋不同科目）
- [ ] 教師後台（題目審核、學生成績追蹤）
- [ ] 學習歷程記錄（每次對局結果存檔）
- [ ] 多人對局 / WebSocket（目前單人單 session）
- [ ] AiSS 主系統（2D 太空站）— 等 InfoLit 穩定後再開

## 四、程式碼統計

| 項目 | 數值 |
|---|---|
| Rust crate 數量 | 4（workspace）+ 1（aiss-import 獨立） |
| 演員 YAML | 15 |
| 騙術 YAML | 8 |
| 題目 YAML | 3 |
| infolit-game 測試 | 33（全通過） |
| Git commits (main) | 7 |

## 五、硬體與測試環境

- **GPU**: NVIDIA GeForce RTX 4060 Ti 16GB
- **Ollama**: 本機 port 22549
- **測試模型**: gemma4:latest（支援 thinking mode）
- **已驗證**: `--doctor` 預檢 + `--think` 模式 + Web SSE 串流 + 18→25 字上限
