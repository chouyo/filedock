# Build FileDock with the skip WebView2 install mode. See build-windows.ps1.
& "$PSScriptRoot/build-windows.ps1" -Type skip
exit $LASTEXITCODE
