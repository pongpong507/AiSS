# Docker 化 Scaffold 程式碼審查報告

> 審查對象：commit `07717d0` (feat: Add Dockerization scaffold for web service)
> 審查日��：2026-04-12

## 結論：目前無法編譯，需要修正後才能使用

---

## 問題清單

### P0 — 無法編譯（必修）

#### 1. 不存在的 import 路徑

**檔案**：`docker-infolit/src/main.rs` 第 8 行、`server/src/main.rs` 第 7 行

```rust
use llm_gateway::llm_gateway::generate_content;
```

**問題**：`llm_gateway` crate 中沒有 `llm_gateway` 子模組，也沒有 `generate_content` 函數。

**正確路徑**：`llm_gateway` 對外公開的模組是：
- `llm_gateway::adapters::OllamaProvider`
- `llm_gateway::provider::LlmProvider`
- `llm_gateway::types::{ChatRequest, ChatResponse, ChatMessage}`
- `llm_gateway::error::LlmError`

#### 2. axum 0.7 API 已變更

**檔案**：`docker-infolit/src/main.rs` 第 23-26 行

```rust
axum::Server::bind(&addr)
    .serve(app.into_make_service())
    .await
    .unwrap();
```

**問題**：`axum::Server` 在 0.7 版已移除。

**正確寫法（axum 0.7）**：
```rust
let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
axum::serve(listener, app).await.unwrap();
```

### P1 — Dockerfile 缺陷（必修）

#### 3. 缺少 builder stage

```dockerfile
FROM debian:bullseye-slim
COPY --from=builder /app/target/release/docker-infolit /usr/local/bin/server
```

**問題**：`COPY --from=builder` 引用了一個不存在的 `builder` stage。需要在前面加上：

```dockerfile
FROM rust:1.80-slim-bullseye AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p docker-infolit
```

#### 4. 缺少執行期依賴

容器用 `debian:bullseye-slim`，但缺少：
- `ca-certificates`（HTTPS 連線需要）
- `libssl3`（reqwest TLS 需要）

```dockerfile
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
```

#### 5. 監聽位址錯誤

```rust
let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
```

**問題**：Docker 容器內監聽 `127.0.0.1` 只接受容器內部連線，外部（包括 host）無法連入。

**修正**：改為 `0.0.0.0`
```rust
let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
```

### P2 — 架構問題（建議）

#### 6. 沒有整合遊戲邏輯

目前 `docker-infolit` 只有一個空的 `/api/generate` endpoint，回傳固定字串。需要整合：
- `infolit-game` crate 的 `GameSession`、`Actor`、`selector`
- WebSocket 支援（遊戲對話是即時的）
- 靜態檔案服務（前端 HTML/JS）

#### 7. 重複檔案

`server/src/main.rs` 與 `docker-infolit/src/main.rs` 內容幾乎相同，且 `server/` 沒有 `Cargo.toml`，建議刪除 `server/` 目錄。

#### 8. 未使用的依賴

`tower-http` 引入了 `fs` feature 但未使用。

---

## 建議的下一步

如果目標是「讓 InfoLit 遊戲可以在網頁上跑」，建議的架構是：

1. **後端**：Axum 0.7 + WebSocket，整合 `infolit-game` crate
2. **前端**：簡單的 HTML + JS 對話介面
3. **Dockerfile**：正確的 multi-stage build
4. **docker-compose.yml**：一鍵啟動（後端 + Ollama）

這部分可以在 Milestone 1 實作，目前的 scaffold 建議先不合併或重寫。
