#!/usr/bin/env bash
# Parrot installer — downloads the latest .app via curl (no quarantine attribute)
# and drops it straight into /Applications, so Gatekeeper never triggers its
# notarization check. Usage:
#
#   curl -fsSL https://raw.githubusercontent.com/ungurenko/parrot/main/install.sh | bash

set -euo pipefail

REPO="${PARROT_REPO:-ungurenko/parrot}"
ARCHIVE_URL="https://github.com/${REPO}/releases/latest/download/Parrot.app.tar.gz"
APP_PATH="/Applications/Parrot.app"
TMP_ARCHIVE="$(mktemp -t parrot-install).tar.gz"

BOLD=$'\033[1m'
DIM=$'\033[2m'
GREEN=$'\033[32m'
BLUE=$'\033[34m'
YELLOW=$'\033[33m'
RED=$'\033[31m'
RESET=$'\033[0m'

info()    { printf "%s==>%s %s\n" "$BLUE"   "$RESET" "$1"; }
success() { printf "%s✓%s %s\n"   "$GREEN"  "$RESET" "$1"; }
warn()    { printf "%s!%s %s\n"   "$YELLOW" "$RESET" "$1"; }
error()   { printf "%s✗%s %s\n"   "$RED"    "$RESET" "$1" >&2; }

cleanup() {
  rm -f "$TMP_ARCHIVE"
}
trap cleanup EXIT

# 1. Sanity checks
if [[ "$(uname -s)" != "Darwin" ]]; then
  error "Parrot работает только на macOS."
  exit 1
fi

if [[ "$(uname -m)" != "arm64" ]]; then
  error "Parrot собран только для Apple Silicon (M1/M2/M3/M4). Intel Mac не поддерживается."
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  error "Требуется curl (обычно предустановлен в macOS)."
  exit 1
fi

if [[ ! -w "/Applications" ]]; then
  error "Нет прав на запись в /Applications. Запусти скрипт от своего пользователя, не через sudo."
  exit 1
fi

printf "%s🦜 Установка Parrot%s\n\n" "$BOLD" "$RESET"

# 2. Remove existing install if present
if [[ -e "$APP_PATH" ]]; then
  # Make sure Parrot isn't running (otherwise replace will fail mid-use).
  if pgrep -f "/Applications/Parrot.app/Contents/MacOS/parrot" >/dev/null 2>&1; then
    info "Закрываю запущенный Parrot..."
    osascript -e 'tell application "Parrot" to quit' >/dev/null 2>&1 || true
    sleep 1
  fi
  info "Удаляю предыдущую версию..."
  rm -rf "$APP_PATH"
fi

# 3. Download latest archive
info "Скачиваю последнюю версию (~72 MB)..."
if ! curl -fL --progress-bar -o "$TMP_ARCHIVE" "$ARCHIVE_URL"; then
  error "Не удалось скачать $ARCHIVE_URL"
  error "Проверь интернет или попробуй позже."
  exit 1
fi

# 4. Extract into /Applications (archive root = Parrot.app/)
info "Распаковываю в /Applications..."
if ! tar -xzf "$TMP_ARCHIVE" -C /Applications; then
  error "Не удалось распаковать архив."
  exit 1
fi

if [[ ! -d "$APP_PATH" ]]; then
  error "Ожидал $APP_PATH после распаковки, но его нет."
  exit 1
fi

# 5. Belt-and-suspenders: strip any quarantine that might have been added
#    (curl itself doesn't add it, but tar can carry over xattrs from the archive).
xattr -cr "$APP_PATH" 2>/dev/null || true

success "Parrot установлен в $APP_PATH"

# 6. Launch
info "Запускаю Parrot..."
open "$APP_PATH"

printf "\n%s🎉 Готово!%s Parrot доступен в Launchpad и папке Программы.\n" "$GREEN" "$RESET"
printf "%sДля обновления запусти ту же команду повторно.%s\n" "$DIM" "$RESET"
