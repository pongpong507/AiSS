---
tags: [adr, infolit, game-loop, architecture]
related:
  - "[[infolit-session]]"
  - "[[actor-pool]]"
  - "[[deception-catalog]]"
  - "[[ADR-003-affinity-system]]"
status: active
last_updated: 2026-04-11
---

# ADR-002：InfoLit 遊戲迴圈設計

## 狀態

**已接受（Accepted）**

## 背景

InfoLit 遊戲需要同時管理多個 LLM agent（3-5 個演員），讓學生在自由對話中練習找出「誰在說謊」。

## 決策

### 核心結構

```
GameSession
├── actors: Vec<Actor>         # 本局演員列表
├── liar_ids: Vec<String>      # 騙子的 ID 列表
├── deceptions: HashMap<ActorId, DeceptionPattern>
├── topic: Topic               # 本局討論主題
├── transcript: Vec<ChatTurn>  # 完整對話紀錄
└── pacing: PacingConfig       # 節奏控制
```

### 遊戲流程

1. `assemble_session()` — affinity 加權挑選演員 + 騙術組合
2. 所有演員各開場一次
3. 互動迴圈（最多 MAX_ROUNDS 回合）：
   - 學生輸入文字 → `session.student_says()`
   - 每個演員依序回應 → `session.actor_respond()`
   - Pacing 延遲（1.5–3.5 秒）模擬真實節奏
4. 學生指控 → `session.score()` 評分 → 揭曉答案

### 對話架構

每個演員有獨立的 system prompt（含騙術指令或誠實指令），對話歷史只包含「學生 ↔ 該演員」的互動（簡化 Milestone 0 實作）。

### NPC 對話全部使用空白輸入框

沒有預設選項，強迫學生自己想「要問什麼」，對應資訊判讀能力培養目標。

## 後果

- 簡化：Milestone 0 不把其他演員發言加進各自 context，降低複雜度
- 未來可升級為「全場演員都能看到彼此發言」的完整多方對話
- `transcript` 留存完整紀錄，可用於老師後台報表

## 原始碼位置

`crates/infolit-game/src/session.rs`、`crates/infolit-game/src/selector.rs`
