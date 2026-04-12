#!/usr/bin/env bash
# 本機啟動腳本：建置 → 預檢 → 開遊戲
# 用法：
#   ./scripts/start-local.sh                       # 預設 qwen2.5:7b
#   MODEL=qwen2.5:14b ./scripts/start-local.sh
#   ./scripts/start-local.sh --no-delay --liars 2  # 任何額外參數會傳給遊戲

set -euo pipefail

MODEL="${MODEL:-qwen2.5:7b}"
OLLAMA_URL="${OLLAMA_URL:-http://localhost:11434}"

cd "$(dirname "$0")/.."

echo "═══════════════════════════════════════════════════════════"
echo " InfoLit 本機啟動腳本"
echo "   模型：$MODEL"
echo "   Ollama：$OLLAMA_URL"
echo "═══════════════════════════════════════════════════════════"
echo

# 1. 建置
echo "[1/3] cargo build (release-debug 中間檔可能要 1-3 分鐘)..."
cargo build -p infolit-game --quiet
echo "  ✅ 建置完成"
echo

# 2. 預檢
echo "[2/3] 執行預檢 (--doctor)..."
cargo run -p infolit-game --quiet -- \
    --doctor \
    --model "$MODEL" \
    --ollama-url "$OLLAMA_URL"
echo

# 3. 開遊戲
echo "[3/3] 啟動 InfoLit CLI..."
echo
exec cargo run -p infolit-game --quiet -- \
    --model "$MODEL" \
    --ollama-url "$OLLAMA_URL" \
    "$@"
