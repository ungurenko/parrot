#!/usr/bin/env bash
# Re-sign Parrot.app and its sidecars with entitlements that allow PyInstaller-bundled
# yt-dlp to dlopen its embedded Python.framework under hardened runtime.
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
ENTITLEMENTS="$PROJECT_ROOT/src-tauri/entitlements.plist"
IDENTITY="${SIGNING_IDENTITY:--}"

if [[ ! -d "$APP_PATH" ]]; then
  echo "sign_macos_bundle: .app not found at $APP_PATH" >&2
  exit 1
fi

if [[ ! -f "$ENTITLEMENTS" ]]; then
  echo "sign_macos_bundle: entitlements file missing at $ENTITLEMENTS" >&2
  exit 1
fi

echo "sign_macos_bundle: re-signing $APP_PATH with identity '$IDENTITY'"

sign() {
  local target="$1"
  [[ -e "$target" ]] || return 0
  xattr -cr "$target" 2>/dev/null || true
  codesign --force --sign "$IDENTITY" \
    --entitlements "$ENTITLEMENTS" \
    --options runtime \
    --timestamp=none \
    "$target"
}

for bin in yt-dlp ffmpeg; do
  sign "$APP_PATH/Contents/MacOS/$bin"
done

sign "$APP_PATH"

echo "sign_macos_bundle: done"
codesign -d --entitlements - "$APP_PATH/Contents/MacOS/yt-dlp" 2>&1 \
  | grep -E '(library-validation|dyld-environment)' || {
    echo "sign_macos_bundle: WARNING — entitlements missing on yt-dlp after re-sign" >&2
    exit 1
  }
