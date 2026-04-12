# AiSS — Ai Space System

AiSS 是一套以 **Rust** 為核心的小學 4-6 年級遊戲式學習平台。核心模組 **InfoLit（資訊判讀遊戲）** 利用 multi-agent LLM，讓學生在「誰在胡說八道」的情境中練習資訊素養。

## InfoLit 遊戲概念

學生進入一間線上討論室，與 3-5 位 AI 角色討論某個主題。其中 1-2 位被秘密指定為「騙子」，會使用特定的欺騙策略（偽造引用、斷章取義、恐懼訴求等）來誤導討論。

學生必須透過追問與觀察，找出誰在說謊並說明理由。

**評分對應教育部《AI 素養手冊》的三色警報系統：**

| 結果 | 判定 |
|---|---|
| 找對騙子 + 理由具體 | 🟢 Green |
| 找對但理由模糊 | 🟡 Yellow |
| 找錯 | 🔴 Red |

## 專案結構

```
AiSS/
├── crates/
│   ├── shared-types/       # 共用型別（Verdict, Topic, Difficulty）
│   ├── llm-gateway/        # LlmProvider trait + Ollama adapter
│   ├── infolit-game/       # 演員池 × 騙術清單 × session × CLI
│   └── aiss-import/        # YAML / CSV 批次匯入工具
├── content/
│   ├── actors/             # 10 位 AI 演員 YAML
│   └── deception-patterns/ # 8 種騙術 YAML
├── docs/architecture/      # ADR（架構決策紀錄）
└── scripts/                # 本機啟動腳本
```

## 核心設計

### LLM Provider 抽象層

所有 LLM 呼叫都經過 `LlmProvider` trait，切換 provider 只需實作一個 adapter：

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, LlmError>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Embedding>, LlmError>;
}
```

目前實作：**OllamaProvider**（本地推論，學生資料不出境）。

### Affinity 加權系統

演員與騙術各有 `affinity` 值（1-12），三個錨點：

```
1────3────6────9────12
   情感  邏輯  權威  魅力型
```

距離越近的組合越容易被配對，但保留 15% 完全隨機通道確保多樣性。

## 快速開始

### 前置需求

- Rust toolchain (>= 1.75)
- [Ollama](https://ollama.com/)

### 安裝與啟動

```bash
# 1. 安裝 Ollama 並下載模型
ollama pull qwen2.5:7b

# 2. Clone 並建置
git clone https://github.com/pongpong507/AiSS.git
cd AiSS
cargo build --workspace

# 3. 環境預檢（確認 Ollama + 模型 + 內容都就緒）
cargo run -p infolit-game -- --doctor

# 4. 開始遊戲
cargo run -p infolit-game -- --model qwen2.5:7b
```

或使用一鍵腳本：

```bash
# Linux / macOS / Git Bash
./scripts/start-local.sh

# PowerShell
.\scripts\start-local.ps1
```

### CLI 參數

```
--model <name>       Ollama 模型名稱（預設 qwen2.5:7b）
--ollama-url <url>   Ollama 服務位置（預設 http://localhost:11434）
--actors <n>         本局演員數量（預設 3）
--liars <n>          騙子數量（預設 1）
--no-delay           關閉回應延遲（加速測試）
--doctor             只做環境預檢，不進入遊戲
--content-dir <path> 內容目錄（預設 ./content）
```

### 遊戲操作

- 直接輸入文字向演員提問
- `/accuse N` — 指控第 N 位成員說謊（例：`/accuse 2`）
- `/quit` — 結束遊戲

### 批次匯入（aiss-import）

```bash
# 驗證演員資料格式（dry-run）
cargo run -p aiss-import -- actors content/actors --dry-run --out-dir ./out

# 匯入騙術資料
cargo run -p aiss-import -- deceptions content/deception-patterns --out-dir ./out

# 支援 .yaml / .csv，副檔名自動偵測
```

### 執行測試

```bash
cargo test --workspace     # 39 個測試
cargo clippy --workspace   # 程式碼品質檢查
```

## 技術堆疊

| 面向 | 選型 |
|---|---|
| 語言 | Rust |
| 非同步 | Tokio |
| LLM | Ollama（本地推論） |
| 序列化 | serde + serde_json + serde_yaml |
| CLI | clap |
| HTTP | reqwest |
| 測試 | cargo test + tokio::test |

## 架構決策紀錄

- [ADR-001](docs/architecture/ADR-001-llm-provider-trait.md) — LlmProvider trait 設計
- [ADR-002](docs/architecture/ADR-002-infolit-game-loop.md) — InfoLit 遊戲迴圈
- [ADR-003](docs/architecture/ADR-003-affinity-system.md) — Affinity 1-12 系統

## Roadmap

- [x] **Milestone 0** — InfoLit CLI spike + LLM gateway + 內容匯入
- [ ] **Milestone 1** — InfoLit Web 版（Axum + WebSocket + React）
- [ ] **Milestone 2** — AiSS 主殼（2D 地圖 + NPC）
- [ ] **Milestone 3** — InfoLit 嵌入 AiSS

## License

MIT
