# FileDock

Windows desktop application for managing file categories and their monitored directories, with cross-application drag-and-drop support.

## Repository Layout

- `app/` — current FileDock desktop application (React + Tauri)
- `web/` — reserved for the future FileDock product website
- `docs/` — shared project documentation
- `.github/` — repository automation and workflows

## Tech Stack

| Layer | Choice |
|---|---|
| Desktop framework | Tauri 2.0 |
| Frontend | React 18 + TypeScript + Vite |
| Styling | Tailwind CSS |
| Drag (cross-app) | `tauri-plugin-drag` (`@crabnebula/tauri-plugin-drag`) |
| Drag sort | `@dnd-kit/core` + `@dnd-kit/sortable` |
| File/dir dialog | `@tauri-apps/plugin-dialog` |
| Clipboard | `@tauri-apps/plugin-clipboard-manager` |
| Updater | `tauri-plugin-updater` |
| Single instance | `tauri-plugin-single-instance` |
| System tray | Tauri built-in (`tray-icon` feature) |
| Local storage | `dirs` + `serde_json` (atomic writes) |
| File scanning | `walkdir` + `glob` + `regex` |
| Concurrency | `tokio::sync::Mutex` (per-category serialization) |
| i18n | Custom (React Context + JSON) |

## Commands

| Command | Description |
|---|---|
| `cd app && npm install` | Install frontend dependencies |
| `cd app && npm run dev` | Start Vite dev server |
| `cd app && npm run tauri dev` | Start Tauri dev (Rust + Vite) |
| `./app/scripts/dev.ps1` | Start Tauri dev (Rust + Vite) from the repository root |
| `cd app && npm run build` | Build frontend for production |
| `cd app && npm run tauri build` | Build production app (default config) |
| `cd app && npm run typecheck` | TypeScript type checking |
| `cd app && npm run lint` | ESLint |
| `cargo check` (in `app/src-tauri/`) | Check Rust compilation |

## Build Variants

Five WebView2 install modes are supported. All use NSIS `currentUser` install mode.

| Type | Config | Script | Size | Offline | Description |
|---|---|---|---|---|---|
| skip | `app/src-tauri/tauri.conf.skip.json` | `app/scripts/build-skip.ps1` | ~3MB | — | Assumes WebView2 already installed; no installation step |
| downloadBootstrapper | `app/src-tauri/tauri.conf.download.json` | `app/scripts/build-download.ps1` | ~3MB | No | Downloads bootstrapper at install time (network required) |
| embedBootstrapper | `app/src-tauri/tauri.conf.embed.json` | `app/scripts/build-embed.ps1` | ~4MB | No | Embeds bootstrapper; downloads runtime at install time |
| offlineInstaller | `app/src-tauri/tauri.conf.offline.json` | `app/scripts/build-offline.ps1` | ~180MB | Yes | Embeds full WebView2 offline installer |
| fixedRuntime | `app/src-tauri/tauri.conf.fixed.json` | `app/scripts/build-fixed.ps1` | ~180MB | Yes | Bundles a fixed/pinned WebView2 runtime |

Build any variant (run inside `app/`, or call the helper script from the repository root):
```powershell
cd app
npx tauri build --config src-tauri/tauri.conf.<type>.json
# or use the helper script from the repo root:
./app/scripts/build-<type>.ps1
```

Build all variants at once:
```powershell
./app/scripts/build-all.ps1
```

Installers are written to `app/src-tauri/target/release/bundle/nsis/` as
`FileDock_<version>_x64-setup-<type>.exe` (e.g. `FileDock_1.0.0_x64-setup-skip.exe`).

### fixedRuntime prerequisites
1. Download fixed WebView2 runtime `.cab` from Microsoft.
2. Extract to `app/src-tauri/webview2-runtime/`.
3. Build via `app/scripts/build-fixed.ps1`.

## Config File Locations

- `%APPDATA%\FileDock\config.json` — categories + session
- `%APPDATA%\FileDock\settings.json` — language, closeAction, columns, windowState

## Icon Generation

```powershell
./app/scripts/generate-icons.ps1                            # expects app/app-icon.png by default
./app/scripts/generate-icons.ps1 -Source src-tauri/icons/icon.png  # custom source, relative to app/
```

Or use Tauri's icon generator directly from `app/`:
```bash
cd app
npx tauri icon path/to/source.png
```

## Key Architecture Notes

- **Drag plugin**: Requires `drag:default` permission in `app/src-tauri/capabilities/default.json`. Without it, drag silently fails.
- **Dark mode**: Applied by default via `document.documentElement.classList.add('dark')`.
- **Config persistence**: All changes written immediately via atomic write (temp file + rename).
- **File scanning**: Per-category serialization via `tokio::sync::Mutex`; prevents concurrent scans of the same category.
- **Window state**: Saved on Move/Resize with 500ms debounce; restored on startup with 4-level monitor matching.
- **Single instance**: Second launch focuses existing window instead of creating a new one.
