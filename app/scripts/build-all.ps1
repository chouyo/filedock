# Build all FileDock Windows variants via build-windows.ps1.
# Each installer gets a type suffix so variants don't overwrite each other.
#
# Usage:
#   .\scripts\build-all.ps1                        # build all 5 variants
#   .\scripts\build-windows.ps1 -Type <type>       # build a single variant (skip | download | embed | offline | fixed)

$ErrorActionPreference = "Stop"

$types   = @("skip", "download", "embed", "offline", "fixed")
$results = @()

foreach ($t in $types) {
    # fixed's missing-runtime prompt must not block an all-build
    & "$PSScriptRoot/build-windows.ps1" -Type $t -NoPrompt
    $status = if ($LASTEXITCODE -eq 0) { "OK" } else { "FAILED" }
    $results += [pscustomobject]@{ Type = $t; Status = $status }
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Build Summary" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
$results | Format-Table -AutoSize

$failed = $results | Where-Object { $_.Status -ne "OK" }
if ($failed) { exit 1 }
