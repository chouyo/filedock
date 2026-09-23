# FileDock Repository

This repository contains the FileDock product sources and supporting project files.

## Directory Layout

- `app/` — the current FileDock desktop application (React + Tauri)
- `web/` — reserved for the future FileDock product website
- `docs/` — shared project documentation, including the user manual
- `.github/` — GitHub Actions workflows and repository automation

## Desktop App Quick Start

```powershell
cd app
npm install
npm run tauri dev
```

From the repository root you can also use the PowerShell helpers in `app/scripts/`, for example:

```powershell
./app/scripts/dev.ps1
./app/scripts/build-skip.ps1
```

## Additional Documentation

- `app/README.md` — desktop app development and build notes
- `web/README.md` — placeholder for the future product website
- `AGENTS.md` — repository-specific guidance for coding agents
