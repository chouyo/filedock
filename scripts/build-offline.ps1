# Build FileDock with offlineInstaller WebView2 install mode
# Result: large installer (~180MB), fully offline WebView2 installation
# Embeds the complete WebView2 offline installer in the bundle

$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)

$variant = "offlineInstaller"
$config  = "src-tauri/tauri.conf.offline.json"

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