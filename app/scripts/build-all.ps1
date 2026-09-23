# Build all FileDock variants by invoking each individual build script.
# Each variant's installer is renamed with a type suffix to avoid overwriting.
#
# Usage:
#   .\scripts\build-all.ps1            # build all 5 variants
#   .\scripts\build-<type>.ps1         # build a single variant (skip | download | embed | offline | fixed)

$ErrorActionPreference = "Stop"
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)

# Each variant delegates to its own build-<type>.ps1 so behavior is shared.
$targets = @(
    @{ Variant = "skip";                 Script = "scripts/build-skip.ps1" },
    @{ Variant = "downloadBootstrapper"; Script = "scripts/build-download.ps1" },
    @{ Variant = "embedBootstrapper";   Script = "scripts/build-embed.ps1" },
    @{ Variant = "offlineInstaller";   Script = "scripts/build-offline.ps1" },
    @{ Variant = "fixedRuntime";        Script = "scripts/build-fixed.ps1" }
)

$results = @()

foreach ($t in $targets) {
    # fixedRuntime's missing-runtime prompt must not block an all-build
    $params = @{}
    if ($t.Variant -eq "fixedRuntime") { $params["NoPrompt"] = $true }

    & $t.Script @params
    if ($LASTEXITCODE -eq 0) {
        $results += [pscustomobject]@{ Variant = $t.Variant; Status = "OK" }
    } else {
        $results += [pscustomobject]@{ Variant = $t.Variant; Status = "FAILED" }
    }
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Build Summary" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
$results | Format-Table -AutoSize

$failed = $results | Where-Object { $_.Status -ne "OK" }
if ($failed) { exit 1 }