# Linux + OpenXR

Horizon VR on Linux uses the same OpenXR session as the Quest build
(`crates/horizon-xr`). The desktop client from
[horizon-rust/horizon](https://github.com/horizon-rust/horizon) is unchanged when
no runtime is selected: it opens a window and plays flat.

## When VR starts

| Situation | Result |
|---|---|
| `HORIZON_XR=0` (also `off` or `false`) | Flat window. |
| `HORIZON_XR=1` (or `on` / `true`) | OpenXR, even if no runtime file was found. |
| `XR_RUNTIME_JSON` set, or `~/.config/openxr/1/active_runtime.json`, or `/etc/xdg/openxr/1/active_runtime.json` | OpenXR. |
| None of the above | Flat window. No attempt to load the OpenXR loader. |

If the loader or the headset is missing, the game logs the reason and continues
with the window. A launch that never reaches input focus writes `xr.pending`
under `HORIZON_HOME` (or `~/.local/share/horizon`); the next launch is flat, then
VR is tried again.

## Build

The game is the same `horizon-client` binary as upstream Linux. It does not link
OpenXR at build time: at startup it loads `libopenxr_loader.so.1` (Khronos loader).
Point `OPENXR_LOADER` at another file if the soname is not on the library path.

```sh
# Same host deps as horizon-rust/horizon (Vulkan, ALSA, udev). The OpenXR loader
# is only needed to run in VR, not to compile. Java 25 is needed to prepare.
cargo build --profile fast -p horizon-client

# Official Mojang files for the pinned version (client, server, libraries, assets),
# sha1-checked, written to .local/mojang and not committed. Then prepares the resources.
cargo run --release -p horizon-prepare -- --download

cargo run --profile fast -p horizon-client -- --resources .local/horizon-client/26.3
```

### aarch64 from a PC

Raspberry Pi OS 64-bit (Pi 500+) and ROCKNIX on a Snapdragon (Retroid Pocket Flip 2)
take the same `aarch64-unknown-linux-gnu` binary. [`packaging/linux/build-arm64.sh`](../packaging/linux/build-arm64.sh)
cross-compiles it with `cargo zigbuild` (glibc 2.31 by default, so a ROCKNIX system
still loads it) and a Debian bookworm sysroot for ALSA and udev. Sound goes through
cpal's ALSA host; there is no PipeWire backend to compile against. Vulkan and Wayland
are loaded at runtime.

```sh
packaging/linux/build-arm64.sh
packaging/linux/build-arm64.sh --glibc 2.36
packaging/linux/build-arm64.sh --no-default-features   # no sound, no gamepads, no sysroot
```

Eye resolution is the runtime's recommended size. `HORIZON_XR_SCALE` (0.4..1.4)
changes that share. On Quest the default scale stays 0.7.

## Runtimes

Monado, SteamVR, WiVRn, and ALVR all speak OpenXR. A Quest over Link or ALVR
usually shows up as an Oculus Touch profile. A Valve Index uses its own profile.
Suggested bindings:

| Profile | Face buttons |
|---|---|
| `/interaction_profiles/oculus/touch_controller` | A B X Y, menu, sticks, triggers, grips |
| `/interaction_profiles/meta/touch_plus_controller` | Same paths (Quest 3 / Pro on some runtimes) |
| `/interaction_profiles/valve/index_controller` | Right A/B are A/B. Left A/B stand in for X/Y. System is the menu button. |
| `/interaction_profiles/khr/simple_controller` | Select clicks (right jump, left drop) and the aim pose. Monado's emulator. |

A profile the runtime does not implement is skipped. The others still bind.

## Controls

Same as Quest (see the README): sticks walk and turn, triggers attack and use,
grips swap hands / debug screen, the menu button pauses. The window stays open
as the winit event loop; frames are submitted to the headset, not to that window.

## Not done yet

- macOS and Windows OpenXR (the crate compiles a stub there).
- Vive wands and WMR controllers (their grip is a click, not a float squeeze).
- A hidden companion window. The desktop window is still created.
- Installing a system `libopenxr_loader.so.1` is left to the distro package
  (`libopenxr-loader1` on Debian, `openxr-loader` elsewhere).
