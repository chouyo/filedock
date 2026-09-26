# Build FileDock with the download WebView2 install mode. See build-windows.ps1.
& "$PSScriptRoot/build-windows.ps1" -Type download
exit $LASTEXITCODE
