# 本機啟動腳本（PowerShell）：建置 → 預檢 → 開遊戲
# 用法：
#   .\scripts\start-local.ps1
#   .\scripts\start-local.ps1 -Model qwen2.5:14b
#   .\scripts\start-local.ps1 -ExtraArgs '--no-delay','--liars','2'

[CmdletBinding()]
param(
    [string]$Model = "qwen2.5:7b",
    [string]$OllamaUrl = "http://localhost:11434",
    [string[]]$ExtraArgs = @()
)

$ErrorActionPreference = "Stop"

Set-Location -Path (Join-Path $PSScriptRoot "..")

Write-Host "═══════════════════════════════════════════════════════════"
Write-Host " InfoLit 本機啟動腳本"
Write-Host "   模型：$Model"
Write-Host "   Ollama：$OllamaUrl"
Write-Host "═══════════════════════════════════════════════════════════"
Write-Host ""

# 1. 建置
Write-Host "[1/3] cargo build (中間檔可能要 1-3 分鐘)..."
cargo build -p infolit-game --quiet
if ($LASTEXITCODE -ne 0) { throw "cargo build 失敗" }
Write-Host "  ✅ 建置完成"
Write-Host ""

# 2. 預檢
Write-Host "[2/3] 執行預檢 (--doctor)..."
cargo run -p infolit-game --quiet -- `
    --doctor `
    --model $Model `
    --ollama-url $OllamaUrl
if ($LASTEXITCODE -ne 0) { throw "預檢失敗" }
Write-Host ""

# 3. 開遊戲
Write-Host "[3/3] 啟動 InfoLit CLI..."
Write-Host ""
$gameArgs = @("--model", $Model, "--ollama-url", $OllamaUrl) + $ExtraArgs
cargo run -p infolit-game --quiet -- @gameArgs
