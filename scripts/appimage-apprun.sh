#!/usr/bin/env bash
set -euo pipefail

appdir="$(dirname "$(readlink -f "$0")")"

# The bundled GLib is older than a newer host's GVFS modules. Keep GIO on the
# matching bundled module set so host plugins cannot load incompatible symbols.
export GIO_MODULE_DIR="$appdir/usr/lib/$(uname -m)-linux-gnu/gio/modules"

exec "$appdir/AppRun.tauri" "$@"
