# Run FileDock in development mode
# Starts Tauri dev (Rust backend + Vite hot-reload frontend)

$ErrorActionPreference = "Stop"
$appRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $appRoot

Write-Host "Starting FileDock (dev)..."
npm run tauri dev
if ($LASTEXITCODE -eq 0) {
    Write-Host "Dev server stopped."
} else {
    Write-Host "Dev server exited with errors."
    exit 1
}
