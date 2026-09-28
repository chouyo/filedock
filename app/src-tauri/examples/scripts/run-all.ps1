# Runs every search benchmark on one dataset (Windows: Spotlight is skipped),
# then prints the comparison.
#
#   .\examples\scripts\run-all.ps1 [-Size 100k] [-Seed 42] [-Extra "--repeat","3"]
#
# Datasets, index state and reports go to %USERPROFILE%\FileDockBench
# (FILEDOCK_BENCH_HOME).
param(
    [string]$Size = "100k",
    [int]$Seed = 42,
    [string[]]$Extra = @()
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..") # app\src-tauri

$examples = "bench_dataset", "bench_current", "bench_seekr", "bench_native", "bench_compare"
$buildArgs = $examples | ForEach-Object { "--example", $_ }
cargo build --release @buildArgs
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$E = "target\release\examples"

function Bench([string]$Bin, [string[]]$BenchArgs) {
    & "$E\$Bin.exe" @BenchArgs @Extra | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Warning "$Bin $($BenchArgs -join ' ') failed" }
}

function Dataset([string[]]$DatasetArgs) {
    & "$E\bench_dataset.exe" @DatasetArgs
    if ($LASTEXITCODE -ne 0) { throw "bench_dataset $($DatasetArgs -join ' ') failed" }
}

$ds = (& "$E\bench_dataset.exe" gen --size $Size --seed $Seed | Select-Object -Last 1).Trim()
if ($LASTEXITCODE -ne 0) { throw "dataset generation failed" }

# Full cycle: S1–S4 + S6, offline mutation, S5 + S6 in a new process, revert.
function Full([string]$Bin, [string[]]$More = @()) {
    Write-Host "=== $Bin $($More -join ' ') ==="
    Bench $Bin (@("--dataset", $ds, "--scenario", "all") + $More)
    Dataset @("mutate", "--dataset", $ds)
    Bench $Bin (@("--dataset", $ds, "--scenario", "s5,s6") + $More)
    Dataset @("revert", "--dataset", $ds)
}

try {
    Dataset @("revert", "--dataset", $ds)
    Full "bench_current"
    Full "bench_seekr"
    Full "bench_native"

    Write-Host "=== bench_native variants ==="
    Bench "bench_native" @("--dataset", $ds, "--scenario", "s1", "--walker", "walkdir")
    Bench "bench_native" @("--dataset", $ds, "--scenario", "s1,s3", "--fts")

    Write-Host "=== bench_native --resume-mode full ==="
    Bench "bench_native" @("--dataset", $ds, "--scenario", "s1")
    Dataset @("mutate", "--dataset", $ds)
    Bench "bench_native" @("--dataset", $ds, "--scenario", "s5,s6", "--resume-mode", "full")
}
finally {
    & "$E\bench_dataset.exe" revert --dataset $ds | Out-Null
}

& "$E\bench_compare.exe" --dataset $ds
