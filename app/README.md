# FileDock App

This directory contains the current FileDock desktop application.

## Development

Run commands from `app/`:

```powershell
npm install
npm run dev
npm run tauri dev
npm run build
npm run typecheck
npm run lint
```

You can also invoke the helper scripts from the repository root or from inside `app/`:

```powershell
# Windows
./app/scripts/dev.ps1
./app/scripts/build-all.ps1
./app/scripts/generate-icons.ps1 -Source src-tauri/icons/icon.png
```

```bash
# macOS
./app/scripts/dev.sh
./app/scripts/build-macos.sh
./app/scripts/generate-icons.sh src-tauri/icons/icon.png
```

## Build Notes

- Tauri configs remain in `app/src-tauri/tauri.conf*.json`.
- Frontend build output remains `app/dist/`, so `frontendDist` stays `../dist` relative to `app/src-tauri/`.
- The fixed WebView2 runtime, when needed, belongs in `app/src-tauri/webview2-runtime/`.
- The 5 Windows build variants only differ in `webviewInstallMode` (how WebView2 is installed), which is a Windows-only concept. macOS uses the system WKWebView, so there is a single `build-macos.sh` script producing an `.app` bundle and `.dmg` installer via `src-tauri/tauri.conf.macos.json`. Pass `--universal` to build a universal binary (Intel + Apple Silicon).
