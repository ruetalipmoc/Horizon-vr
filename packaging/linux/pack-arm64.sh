#!/usr/bin/env bash
# Pack an aarch64 horizon-client and its prepared resources into one archive.
#
# Deploy that archive, not the whole .local directory. .local/mojang is the
# download cache, and saves, options and logs live in $HORIZON_HOME
# (otherwise ~/.local/share/horizon). The prepared tree is
# .local/horizon-client/26.3, and the client loads a resources/ directory
# sitting next to the binary when that directory contains horizon-resources.json.
#
#   packaging/linux/pack-arm64.sh
#   packaging/linux/pack-arm64.sh --binary target/aarch64-unknown-linux-gnu/fast/horizon-client
#   packaging/linux/pack-arm64.sh --appimage
#
# The zip is the file to copy to a Pi 500+ or a ROCKNIX handheld. Extract it
# and run ./horizon-client from a Wayland session. The machine keeps glibc
# (2.36 or newer for the default build), libasound.so.2, libudev.so.1,
# libvulkan.so.1 and libwayland-client.so.0. Pi OS also needs
# mesa-vulkan-drivers. Vulkan, Wayland and ALSA are not packed: a driver from
# the build PC will not run on the device.
#
# --appimage builds horizon-linux-aarch64.AppImage as well. ROCKNIX has no
# FUSE, so the AppImage runtime unpacks the whole image on each launch. Prefer
# the zip there. The image still uses the device's glibc and Vulkan driver.
# ---
set -euo pipefail

cd "$(dirname "$0")/../.."

BINARY="${BINARY:-target/aarch64-unknown-linux-gnu/fast/horizon-client}"
RESOURCES="${RESOURCES:-.local/horizon-client/26.3}"
OUT="${OUT:-target/pack}"
APPIMAGE=0
NAME=horizon-linux-aarch64

usage() {
  sed -n '2,/^# ---$/p' "$0" | sed 's/^# \{0,1\}//; s/^---$//'
  cat <<EOF
Options:
  --binary PATH       aarch64 horizon-client (default: the fast zigbuild output).
  --resources PATH    Prepared tree (default: .local/horizon-client/26.3).
  --out DIR           Where the archives are written (default: target/pack).
  --appimage          Also build an AppImage. The zip is always written.
  -h, --help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary)
      BINARY="${2:?--binary needs a path}"
      shift 2
      ;;
    --resources)
      RESOURCES="${2:?--resources needs a path}"
      shift 2
      ;;
    --out)
      OUT="${2:?--out needs a directory}"
      shift 2
      ;;
    --appimage)
      APPIMAGE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$BINARY" ]]; then
  echo "no binary at $BINARY" >&2
  echo "Build it first: packaging/linux/build-arm64.sh" >&2
  exit 1
fi
if [[ ! -f "$RESOURCES/horizon-resources.json" ]]; then
  echo "no prepared resources at $RESOURCES (horizon-resources.json is missing)" >&2
  echo "On this PC: cargo run --release -p horizon-prepare -- --download" >&2
  exit 1
fi

mkdir -p "$OUT"
zip_path="$OUT/$NAME.zip"
python3 - "$BINARY" "$RESOURCES" "$zip_path" "$NAME" <<'PY'
import os, sys, zipfile
binary, resources, dest, name = sys.argv[1:]
with open(binary, "rb") as fh:
    ident = fh.read(20)
if ident[:4] != b"\x7fELF" or int.from_bytes(ident[18:20], "little") != 183:
    sys.exit(f"{binary} is not an aarch64 ELF")

readme = """Horizon for 64-bit ARM Linux (Raspberry Pi OS and ROCKNIX).

Run ./horizon-client from this folder, in a Wayland session. The resources/
directory next to the binary is the prepared Minecraft tree. Saves, options
and logs go to $HORIZON_HOME, or ~/.local/share/horizon, not into this folder.

The device must provide glibc 2.36 or newer (the default build), plus
libasound.so.2, libudev.so.1, libvulkan.so.1 and libwayland-client.so.0.
On Raspberry Pi OS install mesa-vulkan-drivers. Do not copy Vulkan or ALSA
from the PC that built this archive.

HORIZON_XR=0 keeps a flat window when an OpenXR runtime is installed.
"""

os.makedirs(os.path.dirname(os.path.abspath(dest)), exist_ok=True)
with zipfile.ZipFile(dest, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as z:
    z.write(binary, f"{name}/horizon-client")
    info = zipfile.ZipInfo(f"{name}/README.txt")
    info.external_attr = 0o644 << 16
    z.writestr(info, readme)
    for dirpath, dirnames, filenames in os.walk(resources):
        dirnames.sort()
        for filename in sorted(filenames):
            path = os.path.join(dirpath, filename)
            if os.path.islink(path):
                sys.exit(f"refusing to pack a symlink: {path}")
            rel = os.path.relpath(path, resources)
            z.write(path, f"{name}/resources/{rel}")
print(dest)
PY

echo "Packed $zip_path"
if [[ "$APPIMAGE" -eq 0 ]]; then
  exit 0
fi

# The zip is the portable copy. The image is optional and still leaves glibc,
# Vulkan, Wayland and ALSA on the device.
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
appdir=$work/AppDir
mkdir -p "$appdir/usr/bin/resources" "$appdir/usr/share/icons"
cp -a "$BINARY" "$appdir/usr/bin/horizon-client"
chmod 755 "$appdir/usr/bin/horizon-client"
cp -a "$RESOURCES/." "$appdir/usr/bin/resources/"
cp "$here/../icons/hicolor/512x512/apps/horizon.png" "$appdir/horizon.png"
cp "$appdir/horizon.png" "$appdir/usr/share/icons/horizon.png"
cat > "$appdir/horizon.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Horizon
Comment=Minecraft: Java Edition, rewritten in Rust
Exec=horizon-client
Icon=horizon
Categories=Game;
Terminal=false
EOF
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/horizon-client" "$@"
EOF
chmod 755 "$appdir/AppRun"

tool=${APPIMAGETOOL:-}
if [[ -z "$tool" ]]; then
  tool=$work/appimagetool
  url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
  sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
  curl -fsSL -o "$tool" "$url"
  chmod 755 "$tool"
  echo "$sha256  $tool" | sha256sum -c -
fi
image="$OUT/$NAME.AppImage"
rm -f "$image"
ARCH=aarch64 "$tool" --appimage-extract-and-run --no-appstream "$appdir" "$image"
echo "Packed $image"
