# Generate all platform icons for FileDock from a source image
# Source: squared PNG or SVG (>=1024x1024 recommended) with transparency
# Output: src-tauri/icons/ (regenerates all sizes for Windows/macOS/Linux/iOS/Android)
#
# Usage:
#   ./scripts/generate-icons.ps1                          # expects app/app-icon.png by default
#   ./scripts/generate-icons.ps1 -Source path/to/icon.png

param(
    [string]$Source = ""
)

$ErrorActionPreference = "Stop"
$appRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $appRoot

$defaultSource = Join-Path $appRoot "app-icon.png"
if ([string]::IsNullOrWhiteSpace($Source)) {
    $resolvedSource = $defaultSource
} elseif ([System.IO.Path]::IsPathRooted($Source)) {
    $resolvedSource = $Source
} else {
    $resolvedSource = Join-Path $appRoot $Source
}

if (-not (Test-Path -LiteralPath $resolvedSource)) {
    if ($resolvedSource -eq $defaultSource) {
        Write-Host "Default icon source not found: $defaultSource" -ForegroundColor Yellow
        Write-Host "Add app/app-icon.png or provide -Source relative to app/." -ForegroundColor Yellow
        Write-Host "For example: ./scripts/generate-icons.ps1 -Source src-tauri/icons/icon.png" -ForegroundColor Yellow
    } else {
        Write-Host "Icon source not found: $resolvedSource" -ForegroundColor Red
    }
    exit 1
}

Write-Host "Generating icons from $resolvedSource..."
& npx tauri icon $resolvedSource
if ($LASTEXITCODE -eq 0) {
    Write-Host "Icons generated at src-tauri/icons/." -ForegroundColor Green
} else {
    Write-Host "Icon generation failed." -ForegroundColor Red
    exit 1
}
