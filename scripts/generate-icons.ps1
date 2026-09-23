# Generate all platform icons for FileDock from a source image
# Source: squared PNG or SVG (>=1024x1024 recommended) with transparency
# Output: src-tauri/icons/ (regenerates all sizes for Windows/macOS/Linux/iOS/Android)
#
# Usage:
#   .\scripts\generate-icons.ps1                          # uses ./app-icon.png (default)
#   .\scripts\generate-icons.ps1 -Source path\to\icon.png

param(
    [string]$Source = "./app-icon.png"
)

Write-Host "Generating icons from $Source..."
npx tauri icon $Source
if ($LASTEXITCODE -eq 0) {
    Write-Host "Icons generated at src-tauri/icons/."
} else {
    Write-Host "Icon generation failed."
    exit 1
}