#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Nerfify"
PROCESS_NAME="nerfify"
BUILT_APP="$ROOT_DIR/src-tauri/target/release/bundle/macos/$APP_NAME.app"
APP_BUNDLE="/Applications/$APP_NAME.app"
APP_BINARY="$APP_BUNDLE/Contents/MacOS/$PROCESS_NAME"
BACKUP_APP="$ROOT_DIR/dist/install-backup/$APP_NAME.app"

pkill -x "$PROCESS_NAME" >/dev/null 2>&1 || true
npm --prefix "$ROOT_DIR" run tauri:build -- --bundles app
/usr/bin/codesign --force --deep --sign - "$BUILT_APP"
if [[ -d "$APP_BUNDLE" ]]; then
  /bin/rm -rf "$BACKUP_APP"
  /bin/mkdir -p "$(dirname "$BACKUP_APP")"
  /usr/bin/ditto "$APP_BUNDLE" "$BACKUP_APP"
fi
/bin/rm -rf "$APP_BUNDLE"
/usr/bin/ditto "$BUILT_APP" "$APP_BUNDLE"

case "$MODE" in
  run)
    open "$APP_BUNDLE"
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    open "$APP_BUNDLE"
    /usr/bin/log stream --info --style compact --predicate "process == \"$PROCESS_NAME\""
    ;;
  --telemetry|telemetry)
    open "$APP_BUNDLE"
    /usr/bin/log stream --info --style compact --predicate 'subsystem == "com.nerfify.desktop"'
    ;;
  --verify|verify)
    open "$APP_BUNDLE"
    sleep 1
    pgrep -x "$PROCESS_NAME" >/dev/null
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
