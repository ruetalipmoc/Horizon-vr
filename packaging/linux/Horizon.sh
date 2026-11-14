#!/bin/sh
# ROCKNIX Ports entry. EmulationStation lists only the *.sh files in roms/ports;
# a gamelist.xml line does not add a game. Link this file into that folder
# (the link may point at the copy that sits next to horizon-client):
#
#   ln -s /storage/roms/ports/horizon-linux-aarch64/Horizon.sh /storage/roms/ports/Horizon.sh
#   chmod +x /storage/roms/ports/Horizon.sh /storage/roms/ports/horizon-linux-aarch64/Horizon.sh
#
# Then START, Game Settings, Update Gamelist. Sway tiles a plain window beside
# whatever launched it, which is the split screen (terminal or file manager on
# the left). Fullscreen covers it.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$(readlink -f "$0")")" && pwd)
if [ -x "$here/horizon-client" ]; then
  root=$here
elif [ -x "$here/horizon/horizon-client" ]; then
  root=$here/horizon
else
  echo "horizon-client not found next to $0" >&2
  exit 1
fi
# No headset on the handheld. HORIZON_XR=1 still opts in.
export HORIZON_XR="${HORIZON_XR:-0}"
if [ -n "${SWAYSOCK:-}" ] && command -v swaymsg >/dev/null 2>&1; then
  swaymsg 'for_window [app_id="^horizon$"] floating enable, fullscreen enable, border none' || true
fi
cd "$root"
exec "$root/horizon-client" --fullscreen "$@"
