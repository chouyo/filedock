#!/usr/bin/env bash
# Run FileDock in development mode
# Starts Tauri dev (Rust backend + Vite hot-reload frontend)

set -e
app_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$app_root"

echo "Starting FileDock (dev)..."
if npm run tauri dev; then
    echo "Dev server stopped."
else
    echo "Dev server exited with errors."
    exit 1
fi
