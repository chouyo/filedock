#!/usr/bin/env bash
# Generate all platform icons for FileDock from a source image
# Source: squared PNG or SVG (>=1024x1024 recommended) with transparency
# Output: src-tauri/icons/ (regenerates all sizes for Windows/macOS/Linux/iOS/Android)
#
# Usage:
#   ./scripts/generate-icons.sh                       # expects app/app-icon.png by default
#   ./scripts/generate-icons.sh path/to/icon.png       # custom source, relative to app/

set -e
app_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$app_root"

source_arg="$1"
default_source="$app_root/app-icon.png"

if [[ -z "$source_arg" ]]; then
    resolved_source="$default_source"
elif [[ "$source_arg" == /* ]]; then
    resolved_source="$source_arg"
else
    resolved_source="$app_root/$source_arg"
fi

if [[ ! -f "$resolved_source" ]]; then
    if [[ "$resolved_source" == "$default_source" ]]; then
        echo "Default icon source not found: $default_source"
        echo "Add app/app-icon.png or provide a source path relative to app/."
        echo "For example: ./scripts/generate-icons.sh src-tauri/icons/icon.png"
    else
        echo "Icon source not found: $resolved_source"
    fi
    exit 1
fi

echo "Generating icons from $resolved_source..."
if npx tauri icon "$resolved_source"; then
    echo "Icons generated at src-tauri/icons/."
else
    echo "Icon generation failed."
    exit 1
fi
