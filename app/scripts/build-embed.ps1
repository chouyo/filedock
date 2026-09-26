# Build FileDock with the embed WebView2 install mode. See build-windows.ps1.
& "$PSScriptRoot/build-windows.ps1" -Type embed
exit $LASTEXITCODE
