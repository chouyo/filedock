#!/usr/bin/env bash
# Build FileDock as a macOS .app bundle + .dmg installer
# Result: native macOS app, using the system WKWebView (no WebView2 install modes needed)
#
# Usage:
#   ./scripts/build-macos.sh              # build for the current Mac's architecture
#   ./scripts/build-macos.sh --universal  # build a universal binary (Intel + Apple Silicon)

set -e
app_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$app_root"

variant="macos"
config="src-tauri/tauri.conf.macos.json"

target_args=()
if [[ "$1" == "--universal" ]]; then
    target_args=(--target universal-apple-darwin)
fi

echo ""
echo "=================================================="
echo "Building variant: $variant"
echo "=================================================="

if ! npx tauri build --config "$config" "${target_args[@]}"; then
    echo "FAILED: $variant"
    exit 1
fi

# Report the produced .app bundle and .dmg installer
bundle_root="src-tauri/target"
if [[ "$1" == "--universal" ]]; then
    bundle_root="src-tauri/target/universal-apple-darwin"
fi

app_path=$(find "$bundle_root/release/bundle/macos" -maxdepth 1 -name "*.app" 2>/dev/null | head -n 1)
dmg_path=$(find "$bundle_root/release/bundle/dmg" -maxdepth 1 -name "*.dmg" 2>/dev/null | head -n 1)

if [[ -n "$app_path" ]]; then
    app_size=$(du -sh "$app_path" | cut -f1)
    echo "OK: $variant -> $app_path ($app_size)"
fi
if [[ -n "$dmg_path" ]]; then
    dmg_size=$(du -sh "$dmg_path" | cut -f1)
    echo "OK: $variant -> $dmg_path ($dmg_size)"
fi
if [[ -z "$app_path" && -z "$dmg_path" ]]; then
    echo "Build succeeded but no bundle found in $bundle_root/release/bundle/"
fi
