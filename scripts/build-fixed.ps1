# Build FileDock with fixedRuntime WebView2
# Result: large installer (~180MB), fully offline
# Prerequisite: Extract fixed WebView2 runtime to src-tauri/webview2-runtime/
#
# Usage:
#   .\scripts\build-fixed.ps1            # prompts if runtime is missing
#   .\scripts\build-fixed.ps1 -NoPrompt  # skips the prompt (for build-all / CI)

param([switch]$NoPrompt)

$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)

$variant = "fixedRuntime"
$config  = "src-tauri/tauri.conf.fixed.json"

# Prerequisite check: fixed WebView2 runtime must be present
$runtimeDir   = "src-tauri/webview2-runtime"
$runtimeReady = (Test-Path $runtimeDir) -and
                ((Get-ChildItem $runtimeDir -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0)
if (-not $runtimeReady) {
    Write-Host "WARNING: src-tauri/webview2-runtime/ is empty or missing." -ForegroundColor Yellow
    Write-Host "Download fixed WebView2 runtime from:" -ForegroundColor Yellow
    Write-Host "  https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download-section" -ForegroundColor Yellow
    Write-Host "Extract the .cab file to src-tauri/webview2-runtime/" -ForegroundColor Yellow
    if ($NoPrompt) {
        Write-Host "Continuing anyway (-NoPrompt)..." -ForegroundColor Yellow
    } else {
        $continue = Read-Host "Continue anyway? (y/N)"
        if ($continue -ne "y") { exit 1 }
    }
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Building variant: $variant" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

& npx tauri build --config $config
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: $variant" -ForegroundColor Red
    exit 1
}

# Rename installer with a variant suffix so variants don't overwrite each other
$nsisDir = "src-tauri/target/release/bundle/nsis"
$baseExe = Get-ChildItem -Path $nsisDir -Filter "FileDock_*_x64-setup.exe" -ErrorAction SilentlyContinue |
           Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($baseExe) {
    $version  = ([regex]::Match($baseExe.Name, 'FileDock_(.+?)_x64-setup\.exe')).Groups[1].Value
    $destFile = Join-Path $nsisDir ("FileDock_${version}_x64-setup-${variant}.exe")
    Copy-Item -LiteralPath $baseExe.FullName -Destination $destFile -Force
    $sizeMB = [math]::Round((Get-Item -LiteralPath $destFile).Length / 1MB, 1)
    Write-Host "OK: $variant -> $destFile ($sizeMB MB)" -ForegroundColor Green
} else {
    Write-Host "Build succeeded but no installer found in $nsisDir" -ForegroundColor Yellow
}