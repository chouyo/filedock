# Build one FileDock Windows installer variant.
# Result: src-tauri/target/release/bundle/nsis/FileDock_<version>_x64-setup-<type>.exe
#
# Usage:
#   .\scripts\build-windows.ps1 -Type skip       # ~3MB, assumes WebView2 is already installed
#   .\scripts\build-windows.ps1 -Type download   # ~3MB, downloads bootstrapper at install time (default config)
#   .\scripts\build-windows.ps1 -Type embed      # ~4MB, embeds bootstrapper, downloads runtime at install time
#   .\scripts\build-windows.ps1 -Type offline    # ~180MB, embeds full WebView2 offline installer
#   .\scripts\build-windows.ps1 -Type fixed      # ~180MB, bundles fixed runtime from src-tauri/webview2-runtime/
#   .\scripts\build-windows.ps1 -Type fixed -NoPrompt  # skips the missing-runtime prompt (for build-all / CI)

param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("skip", "download", "embed", "offline", "fixed")]
    [string]$Type,
    [switch]$NoPrompt
)

$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)

# download uses the default tauri.conf.json, so it has no override config
$configs = @{
    skip     = "src-tauri/tauri.conf.skip.json"
    download = $null
    embed    = "src-tauri/tauri.conf.embed.json"
    offline  = "src-tauri/tauri.conf.offline.json"
    fixed    = "src-tauri/tauri.conf.fixed.json"
}
$config = $configs[$Type]

# Prerequisite check: fixed WebView2 runtime must be present
if ($Type -eq "fixed") {
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
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Building variant: $Type" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

if ($config) {
    & npx tauri build --config $config
} else {
    & npx tauri build
}
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: $Type" -ForegroundColor Red
    exit 1
}

# Rename installer with a type suffix so variants don't overwrite each other
$nsisDir = "src-tauri/target/release/bundle/nsis"
$baseExe = Get-ChildItem -Path $nsisDir -Filter "FileDock_*_x64-setup.exe" -ErrorAction SilentlyContinue |
           Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $baseExe) {
    Write-Host "Build succeeded but no installer found in $nsisDir" -ForegroundColor Red
    exit 1
}
$version  = ([regex]::Match($baseExe.Name, 'FileDock_(.+?)_x64-setup\.exe')).Groups[1].Value
$destFile = Join-Path $nsisDir ("FileDock_${version}_x64-setup-${Type}.exe")
Move-Item -LiteralPath $baseExe.FullName -Destination $destFile -Force
$sizeMB = [math]::Round((Get-Item -LiteralPath $destFile).Length / 1MB, 1)
Write-Host "OK: $Type -> $destFile ($sizeMB MB)" -ForegroundColor Green
exit 0
