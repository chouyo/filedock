# Build FileDock with the offline WebView2 install mode. See build-windows.ps1.
& "$PSScriptRoot/build-windows.ps1" -Type offline
exit $LASTEXITCODE
