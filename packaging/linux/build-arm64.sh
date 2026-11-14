#!/usr/bin/env bash
# Cross-compile horizon-client for 64-bit ARM Linux from an x86_64 (or any) PC.
# One binary for both:
#   - Raspberry Pi 500+ on Raspberry Pi OS 64-bit (Raspbian, VideoCore VII / V3DV)
#   - Retroid Pocket Flip 2 (Snapdragon SM8250) on ROCKNIX (Adreno 650 / Turnip)
#
# Rust compiles the target itself (`--target`). What Go would not ask for, and this
# script sets up, is the linker and the two C libraries the default features link:
#   libasound  (cpal, the only Linux sound backend; PipeWire sits under ALSA)
#   libudev    (gilrs, hidapi, and the DualSense haptics path, which also uses cpal)
# Vulkan and Wayland are loaded at runtime, not at link time. OpenXR too
# (libopenxr_loader.so.1); the flat window is the default. See docs/linux-openxr.md.
#
# zig (via cargo-zigbuild) is the linker. It also picks the oldest glibc the binary
# may require. The ALSA and udev shared libraries come from a Debian arm64 sysroot,
# and they must not need a newer glibc than that suffix: ld.lld checks them
# (--no-allow-shlib-undefined). Bookworm's libasound.so and libudev.so reference
# GLIBC_2.34 (pthread_once, dlopen, stat64), so the default suffix is 2.36.
# Do not pass --sysroot to the Rust linker: that would replace zig's glibc.
#
#   packaging/linux/build-arm64.sh
#   packaging/linux/build-arm64.sh --debian bullseye
#   packaging/linux/build-arm64.sh --no-default-features
#
# --debian bullseye builds a glibc 2.31 sysroot and links with .2.31, for a ROCKNIX
# older than 2.36. The existing $HOME/arm64-sysroot is bookworm: pass another
# --sysroot, do not reuse it. A 2.36 binary still runs on the Pi 500+.
#
# --no-default-features skips sound and controllers, so no sysroot is required.
# Keeping either feature still compiles alsa-sys: horizon-dualsense depends on cpal
# even when the client's audio feature is off.
#
# First-time tools, if they are not already on PATH:
#   cargo install --locked cargo-zigbuild
#   rustup target add aarch64-unknown-linux-gnu
#   pip3 install --user ziglang && export PATH="$HOME/.local/bin:$PATH"
#   # or unpack https://ziglang.org/download/ (0.15 or 0.16) and link zig onto PATH
#   # apt's zig is usually too old. Avoid it.
#
# The binary lands in target/aarch64-unknown-linux-gnu/<profile>/horizon-client.
# Copy it and a prepared resource directory (.local/horizon-client/26.3, made on
# the PC with `cargo run --release -p horizon-prepare -- --download`) onto the
# device, then from a Wayland session:
#   HORIZON_XR=0 ./horizon-client --resources /path/to/horizon-client/26.3
# Runtime libraries, already on Pi OS and on the ROCKNIX-SM8250 image:
#   libasound.so.2 libudev.so.1 libvulkan.so.1 libwayland-client.so.0
# Pi OS also needs mesa-vulkan-drivers. On the Flip, start the binary from the
# Wayland session, not from an SSH shell with no display.
# ---
set -euo pipefail

cd "$(dirname "$0")/../.."

GLIBC="${GLIBC:-}"
DEBIAN="${DEBIAN:-bookworm}"
SYSROOT="${SYSROOT:-$HOME/arm64-sysroot}"
PROFILE="${PROFILE:-fast}"
NO_DEFAULT=0
SKIP_SYSROOT=0

usage() {
  sed -n '2,/^# ---$/p' "$0" | sed 's/^# \{0,1\}//; s/^---$//'
  cat <<EOF
Options:
  --glibc VERSION          Minimum glibc. Default follows --debian (bookworm: 2.36).
  --debian SUITE           Sysroot suite when it is created (default: bookworm).
  --sysroot DIR            Debian arm64 sysroot (default: \$HOME/arm64-sysroot).
  --profile NAME           Cargo profile (default: fast).
  --no-default-features    No ALSA and no udev. No sound, no gamepads. No sysroot.
  --skip-sysroot           Do not create the sysroot; it must already contain the .pc files.
  -h, --help               Show this text.
EOF
}

# True when dotted version $1 is strictly older than $2.
version_lt() {
  local a1 a2 b1 b2
  IFS=. read -r a1 a2 _ <<<"${1}.0"
  IFS=. read -r b1 b2 _ <<<"${2}.0"
  if ((10#$a1 < 10#$b1)); then return 0; fi
  if ((10#$a1 > 10#$b1)); then return 1; fi
  ((10#$a2 < 10#$b2))
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --glibc)
      GLIBC="${2:?--glibc needs a version, for example 2.36}"
      shift 2
      ;;
    --debian)
      DEBIAN="${2:?--debian needs a suite, for example bookworm or bullseye}"
      shift 2
      ;;
    --sysroot)
      SYSROOT="${2:?--sysroot needs a directory}"
      shift 2
      ;;
    --profile)
      PROFILE="${2:?--profile needs a name}"
      shift 2
      ;;
    --no-default-features)
      NO_DEFAULT=1
      shift
      ;;
    --skip-sysroot)
      SKIP_SYSROOT=1
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

if ! command -v zig >/dev/null 2>&1; then
  echo "zig is not on PATH. cargo-zigbuild does not install it." >&2
  echo "  pip3 install --user ziglang && export PATH=\"\$HOME/.local/bin:\$PATH\"" >&2
  echo "  or unpack a 0.15/0.16 build from https://ziglang.org/download/" >&2
  exit 1
fi
if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  echo "cargo-zigbuild is not on PATH. Install it with:" >&2
  echo "  cargo install --locked cargo-zigbuild" >&2
  exit 1
fi
if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required (the aarch64-unknown-linux-gnu target)." >&2
  exit 1
fi

if [[ -z "$GLIBC" ]]; then
  case "$DEBIAN" in
    bullseye) GLIBC=2.31 ;;
    *) GLIBC=2.36 ;;
  esac
fi

rustup target add aarch64-unknown-linux-gnu

TARGET="aarch64-unknown-linux-gnu.${GLIBC}"

if [[ "$NO_DEFAULT" -eq 0 ]]; then
  if ! command -v pkg-config >/dev/null 2>&1; then
    echo "pkg-config is required to find libasound and libudev. Install it, or pass --no-default-features." >&2
    exit 1
  fi
  alsa_pc="$SYSROOT/usr/lib/aarch64-linux-gnu/pkgconfig/alsa.pc"
  udev_pc="$SYSROOT/usr/lib/aarch64-linux-gnu/pkgconfig/libudev.pc"
  if [[ ! -f "$alsa_pc" || ! -f "$udev_pc" ]]; then
    if [[ "$SKIP_SYSROOT" -eq 1 ]]; then
      echo "sysroot $SYSROOT is missing alsa.pc or libudev.pc." >&2
      exit 1
    fi
    if ! command -v sudo >/dev/null 2>&1 || ! command -v apt-get >/dev/null 2>&1; then
      echo "Need a Debian/Ubuntu host with sudo to build the arm64 sysroot, or pass --sysroot." >&2
      exit 1
    fi
    echo "Installing the arm64 sysroot into $SYSROOT (Debian $DEBIAN)."
    sudo apt-get install -y debootstrap qemu-user-static pkg-config
    if [[ ! -d "$SYSROOT/usr/bin" ]]; then
      sudo mkdir -p "$(dirname "$SYSROOT")"
      sudo debootstrap --arch=arm64 "$DEBIAN" "$SYSROOT" http://deb.debian.org/debian
    fi
    sudo chroot "$SYSROOT" apt-get update
    sudo chroot "$SYSROOT" apt-get install -y libasound2-dev libudev-dev
  fi
  if [[ ! -f "$alsa_pc" || ! -f "$udev_pc" ]]; then
    echo "sysroot install finished but $alsa_pc is still missing." >&2
    exit 1
  fi

  if [[ -f "$SYSROOT/etc/os-release" ]] && grep -Eq 'VERSION_CODENAME=(bookworm|trixie)' "$SYSROOT/etc/os-release"; then
    if version_lt "$GLIBC" 2.34; then
      echo "This sysroot's libasound.so and libudev.so need GLIBC_2.34." >&2
      echo "zig is targeting $GLIBC, so the link fails on pthread_once@GLIBC_2.34, stat64@GLIBC_2.33 and the rest." >&2
      echo "Use the same sysroot with a matching suffix:" >&2
      echo "  packaging/linux/build-arm64.sh --glibc 2.36" >&2
      echo "A glibc 2.31 binary needs a bullseye sysroot, not this one:" >&2
      echo "  packaging/linux/build-arm64.sh --debian bullseye --sysroot \"\$HOME/arm64-sysroot-bullseye\"" >&2
      exit 1
    fi
  fi

  export PKG_CONFIG_ALLOW_CROSS=1
  export PKG_CONFIG_SYSROOT_DIR="$SYSROOT"
  export PKG_CONFIG_LIBDIR="$SYSROOT/usr/lib/aarch64-linux-gnu/pkgconfig:$SYSROOT/usr/lib/pkgconfig:$SYSROOT/usr/share/pkgconfig"
  export PKG_CONFIG_PATH="$PKG_CONFIG_LIBDIR"
  # hidapi compiles a C file. cargo-zigbuild passes -nostdinc, so say where libudev.h is.
  # Do not put the sysroot on the Rust link line (RUSTFLAGS --sysroot): zig's glibc must stay.
  export CFLAGS_aarch64_unknown_linux_gnu="-isystem $SYSROOT/usr/include"

  alsa_libs="$(pkg-config --libs alsa)"
  udev_libs="$(pkg-config --libs libudev)"
  case "$alsa_libs" in
    *"$SYSROOT"*) ;;
    *)
      echo "pkg-config --libs alsa did not use the sysroot: $alsa_libs" >&2
      exit 1
      ;;
  esac
  case "$udev_libs" in
    *"$SYSROOT"*) ;;
    *)
      echo "pkg-config --libs libudev did not use the sysroot: $udev_libs" >&2
      exit 1
      ;;
  esac
  echo "ALSA: $alsa_libs"
  echo "udev: $udev_libs"
fi

args=(--profile "$PROFILE" -p horizon-client --target "$TARGET" --locked)
if [[ "$NO_DEFAULT" -eq 1 ]]; then
  args+=(--no-default-features)
fi

echo "zig $(zig version) -> $TARGET (profile $PROFILE)"
cargo zigbuild "${args[@]}"

# cargo-zigbuild strips the glibc suffix before calling cargo, so the directory
# has no ".2.31". The profile name is the directory (fast, release, debug).
bin="target/aarch64-unknown-linux-gnu/${PROFILE}/horizon-client"
if [[ ! -f "$bin" ]]; then
  echo "build finished but $bin is missing" >&2
  exit 1
fi
echo
echo "Built $bin"
echo "Pack it with the prepared resources:"
echo "  packaging/linux/pack-arm64.sh --binary $bin"
echo "The archive extracts to a folder whose horizon-client finds resources/ beside it."
if [[ "$NO_DEFAULT" -eq 1 ]]; then
  echo "This binary has no sound and no gamepads (--no-default-features)."
fi
