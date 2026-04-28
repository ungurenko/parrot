#!/usr/bin/env bash
# Re-sign Parrot.app and its sidecars. The main app keeps only the WebKit
# hardened-runtime allowances, while PyInstaller-bundled yt-dlp gets the
# narrower entitlements it needs to dlopen its embedded Python.framework.
#
# Without this, child yt-dlp fails with:
#   "mapping process and mapped file (non-platform) have different Team IDs"
#
# Runs after `tauri build`. Signs sidecars first, then the .app, so the outer
# signature seals the already-signed inner resources.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_PATH="${1:-$PROJECT_ROOT/src-tauri/target/release/bundle/macos/Parrot.app}"
APP_ENTITLEMENTS="$PROJECT_ROOT/src-tauri/entitlements.plist"
YTDLP_ENTITLEMENTS="$PROJECT_ROOT/src-tauri/entitlements-yt-dlp.plist"
IDENTITY="${SIGNING_IDENTITY:--}"

if [[ ! -d "$APP_PATH" ]]; then
  echo "sign_macos_bundle: .app not found at $APP_PATH" >&2
  exit 1
fi

if [[ ! -f "$APP_ENTITLEMENTS" ]]; then
  echo "sign_macos_bundle: app entitlements file missing at $APP_ENTITLEMENTS" >&2
  exit 1
fi

if [[ ! -f "$YTDLP_ENTITLEMENTS" ]]; then
  echo "sign_macos_bundle: yt-dlp entitlements file missing at $YTDLP_ENTITLEMENTS" >&2
  exit 1
fi

echo "sign_macos_bundle: re-signing $APP_PATH with identity '$IDENTITY'"

sign_plain() {
  local target="$1"
  [[ -e "$target" ]] || return 0
  xattr -cr "$target" 2>/dev/null || true
  codesign --force --sign "$IDENTITY" \
    --options runtime \
    --timestamp=none \
    "$target"
}

sign_with_entitlements() {
  local target="$1"
  local entitlements="$2"
  [[ -e "$target" ]] || return 0
  xattr -cr "$target" 2>/dev/null || true
  codesign --force --sign "$IDENTITY" \
    --entitlements "$entitlements" \
    --options runtime \
    --timestamp=none \
    "$target"
}

sign_with_entitlements "$APP_PATH/Contents/MacOS/yt-dlp" "$YTDLP_ENTITLEMENTS"
sign_plain "$APP_PATH/Contents/MacOS/ffmpeg"

sign_with_entitlements "$APP_PATH" "$APP_ENTITLEMENTS"

echo "sign_macos_bundle: done"
codesign -d --entitlements - "$APP_PATH/Contents/MacOS/yt-dlp" 2>&1 \
  | grep -E '(library-validation|dyld-environment)' || {
    echo "sign_macos_bundle: WARNING — entitlements missing on yt-dlp after re-sign" >&2
    exit 1
  }
