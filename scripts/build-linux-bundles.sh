#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Tauri copies the cached AppRun into AppRun.wrapped without changing its mode.
# Keep a verified, executable copy in a build-local cache so the final image is
# usable by users other than the one who built it.
export XDG_CACHE_HOME="$PWD/target/appimage-cache"
case "$(uname -m)" in
  x86_64)
    arch=x86_64
    expected_sha256=f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f
    ;;
  aarch64)
    arch=aarch64
    expected_sha256=072f17c0895a85c490282fe5395c5007e5fc75da727e553b3b8fb680feb11578
    ;;
  *)
    echo "Unsupported Linux architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
apprun="$XDG_CACHE_HOME/tauri/AppRun-$arch"
mkdir -p "$(dirname "$apprun")"

if [[ ! -f "$apprun" ]] || [[ "$(sha256sum "$apprun" | cut -d ' ' -f 1)" != "$expected_sha256" ]]; then
  download=$(mktemp "${apprun}.XXXXXX")
  trap 'rm -f "$download"' EXIT
  curl --fail --location --retry 3 --silent --show-error \
    --output "$download" \
    "https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-$arch"
  actual_sha256=$(sha256sum "$download" | cut -d ' ' -f 1)
  if [[ "$actual_sha256" != "$expected_sha256" ]]; then
    echo "AppRun download checksum mismatch: $actual_sha256" >&2
    exit 1
  fi
  mv "$download" "$apprun"
fi

chmod 0755 "$apprun"
pnpm tauri build --bundles deb,appimage --no-sign \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'

image_dir="$PWD/target/release/bundle/appimage"
appdir="$image_dir/StrawberryDisk.AppDir"
gio_modules="$appdir/usr/lib/$arch-linux-gnu/gio/modules"
plugin="$XDG_CACHE_HOME/tauri/linuxdeploy-plugin-appimage.AppImage"
bundle=("$image_dir"/*.AppImage)
if [[ ! -d "$gio_modules" || ! -x "$plugin" || ${#bundle[@]} -ne 1 || ! -f "${bundle[0]}" ]]; then
  echo "AppImage staging is incomplete; cannot isolate bundled GIO modules" >&2
  exit 1
fi

# Keep the generated launcher as a delegate; the project-owned entry point
# prevents newer host GVFS plugins from loading against the bundled GLib.
cp "$appdir/AppRun" "$appdir/AppRun.tauri"
restore_apprun() {
  mv "$appdir/AppRun.tauri" "$appdir/AppRun"
}
trap restore_apprun EXIT
install -m 0755 scripts/appimage-apprun.sh "$appdir/AppRun"
(
  cd "$image_dir"
  ARCH="$arch" "$plugin" --appimage-extract-and-run --appdir="$appdir"
)
mv "$image_dir/StrawberryDisk-$arch.AppImage" "${bundle[0]}"
restore_apprun
trap - EXIT

python3 scripts/check-linux-bundle.py
