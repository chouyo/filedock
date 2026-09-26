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

| Type | Config | Size | Offline | Description |
|---|---|---|---|---|
| skip | `app/src-tauri/tauri.conf.skip.json` | ~3MB | — | Assumes WebView2 already installed; no installation step |
| download | `app/src-tauri/tauri.conf.json` (default) | ~3MB | No | Downloads bootstrapper at install time (network required) |
| embed | `app/src-tauri/tauri.conf.embed.json` | ~4MB | No | Embeds bootstrapper; downloads runtime at install time |
| offline | `app/src-tauri/tauri.conf.offline.json` | ~180MB | Yes | Embeds full WebView2 offline installer |
| fixed | `app/src-tauri/tauri.conf.fixed.json` | ~180MB | Yes | Bundles a fixed/pinned WebView2 runtime |

All variants are built by `app/scripts/build-windows.ps1`, which also renames the installer with the type suffix.
`app/scripts/build-<type>.ps1` are thin wrappers around it. CI (`.github/workflows/build.yml`) uses the same script.

Build any variant (from the repository root):
```powershell
./app/scripts/build-windows.ps1 -Type <type>
# or the per-type wrapper:
./app/scripts/build-<type>.ps1
```

Build all variants at once:
```powershell
./app/scripts/build-all.ps1
```

Installers are written to `app/src-tauri/target/release/bundle/nsis/` as
`FileDock_<version>_x64-setup-<type>.exe` (e.g. `FileDock_1.0.0_x64-setup-skip.exe`).

### fixed prerequisites
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
- **Keyboard shortcuts**: Browser default shortcuts are disabled. Release builds turn off WebView2 browser accelerator keys natively (`app/src-tauri/src/webview.rs`); `app/src/lib/shortcuts.ts` blocks every Ctrl/Cmd/Alt/F-key combination not in its `WHITELIST`. Add new shortcuts to the whitelist. Dev builds keep F5 and F12.
- **Single instance**: Second launch focuses existing window instead of creating a new one.
