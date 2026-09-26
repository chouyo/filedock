# Build FileDock with the fixed WebView2 runtime. See build-windows.ps1.
#
# Usage:
#   .\scripts\build-fixed.ps1            # prompts if runtime is missing
#   .\scripts\build-fixed.ps1 -NoPrompt  # skips the prompt (for build-all / CI)

param([switch]$NoPrompt)

& "$PSScriptRoot/build-windows.ps1" -Type fixed -NoPrompt:$NoPrompt
exit $LASTEXITCODE
