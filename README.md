<p align="center">
  <img src="docs/media/icon-256.png" width="128" alt="Horizon VR">
</p>

<h1 align="center">Horizon VR</h1>

<p align="center">
  Standalone VR for <b>Meta Quest 3</b>, on Horizon: Minecraft: Java Edition 26.3 rewritten in Rust
</p>

<p align="center">
  <a href="https://github.com/Boboetto/Horizon-vr/releases/latest"><img src="https://img.shields.io/github/v/release/Boboetto/Horizon-vr?label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Meta%20Quest-3%20only-1C1E20" alt="Meta Quest 3 only">
  <img src="https://img.shields.io/badge/Minecraft-Java%2026.3-5D9C3F" alt="Minecraft Java 26.3">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="License MIT"></a>
</p>

> **Unofficial fan project.** Not affiliated with, endorsed by or connected to Mojang, Microsoft or Meta.
> You need to **own Minecraft: Java Edition**. This repository and its releases contain **no Mojang files**: the app
> downloads them from Mojang on the headset.

Horizon VR is a standalone APK for the **Meta Quest 3** (for now the only supported headset): no PC, no streaming.
It plays Minecraft: Java Edition 26.3 in VR, with motion controls in the spirit of Vivecraft and QuestCraft.

## Features

- Stereo VR with head tracking; both arms follow the Touch controllers, items in the hands
- Hotbar on the left forearm: hover it with the right controller and pull the trigger to pick a slot
- A crosshair for each hand, lying on the face of the block it aims at
- The left trigger uses and places along the left hand while the right trigger attacks along the right one
- Gestures: swing a sword at a mob, strike blocks with a tool to mine, bring food to the mouth to eat,
  arm strokes to swim where you look, two-handed bow
- Menus on a panel in the room with a pointer for each hand, 360° title panorama
- Walk where you look with the left stick, turn with the right one

## Controls

| Input | In play | In a menu |
|---|---|---|
| Left stick | walk (where you look) | – |
| Right stick | turn | scroll |
| Right trigger | attack / mine / pick a wrist slot | click with the right pointer |
| Left trigger | use / place along the left hand | click with the left pointer |
| A | jump | confirm |
| B | – | back |
| Y | inventory | close the inventory |
| X | drop one item | drop one item (carried stack or slot) |
| X held + Y | drop the rest of the stack | drop the rest of the stack |
| Left grip | swap the item with the off hand | – |
| Both grips | debug screen (F3) | – |
| Right grip | – | place one item of the carried stack; with the trigger: quick move |
| Left / right stick click | sprint / sneak | – |
| Menu button | pause | – |

## Install on the Meta Quest 3

1. Turn on **developer mode** for your Quest 3: in the **Meta Horizon** app on your phone → Menu → Devices →
   your headset → Headset settings → Developer mode (Meta asks you to create a free developer account once).
2. Download **`Horizon-VR-v*.apk`** from the [latest release](https://github.com/Boboetto/Horizon-vr/releases/latest).
3. Install it with **SideQuest**: connect the headset to the computer with its USB-C cable, accept "Allow USB debugging"
   in the headset, then drag the APK onto the SideQuest window.
   (Or drag it onto **Meta Quest Developer Hub**.)
4. In the headset, open **App Library → Unknown sources → Horizon VR**.

## First start

Press **Download game files** and wait a few minutes, on Wi-Fi:

1. the headset downloads the official Minecraft: Java Edition 26.3 files from Mojang (about 700 MB);
2. Horizon VR prepares its game data on the headset itself, with a Java runtime built into the app (about a minute);
3. the game starts. Next time it opens straight away.

No computer needed after installing. You need to own Minecraft: Java Edition.

## In italiano

1. **Installa:** attiva la modalità sviluppatore (app Meta Horizon sul telefono → Dispositivi → visore →
   Impostazioni → Modalità sviluppatore) e installa l'APK dell'ultima release trascinandolo su **SideQuest**.
2. **Apri** Horizon VR da **Libreria → Origini sconosciute**.
3. **Premi "Download game files"** e aspetta qualche minuto col Wi-Fi: il visore scarica i file ufficiali da Mojang,
   prepara da solo i dati di gioco e avvia Horizon VR. Nessun PC necessario. Serve possedere Minecraft: Java Edition.

## Building

```sh
OPENXR_LOADER=/path/to/libopenxr_loader.so ANDROID_HOME=~/android/sdk ./packaging/android/build_apk.sh
```

`libopenxr_loader.so` is the Khronos OpenXR loader for Android (`org.khronos.openxr:openxr_loader_for_android`,
Apache 2.0). For the first start to prepare the game data on the headset, set `HORIZON_APK_ASSETS` to a folder with
`horizon-jre/` (a Java 25 runtime for Android arm64, plus the NDK's `libc++_shared.so` in its `lib/`) and
`horizon-prepare/` (`research-lock.json`, `mods/horizon/src/test/java/dev/horizonlab/parity/` and the compiled
`exporters/` from `horizon-prepare compile-exporters`). The VR code is in `crates/horizon-xr`, the renderer's `render_xr`, and
`crates/horizon-client/src/game/xr_input.rs` (Android key and motion events stay in
`android_input.rs`).

## Linux (OpenXR)

Desktop headsets use the same session as Quest: Monado, SteamVR, WiVRn, ALVR, or
any other OpenXR runtime. The flat window (the path that
[horizon-rust/horizon](https://github.com/horizon-rust/horizon) already runs on Linux)
stays the default unless a runtime is configured. Details, build, and controls:
[docs/linux-openxr.md](docs/linux-openxr.md).

```sh
# runtime already active (or set XR_RUNTIME_JSON / HORIZON_XR=1)
cargo run -p horizon-client --release
```

## Credits

- **Horizon**, Minecraft: Java Edition rewritten in Rust, by its authors (MIT)
- **Horizon for Android** by [Rayan-XOO](https://github.com/Rayan-XOO/horizon) (MIT), which this project forks
- The Khronos OpenXR loader (Apache 2.0)
- The Java 25 runtime for Android from [AngelAuraMC/angelauramc-openjdk-build](https://github.com/AngelAuraMC/angelauramc-openjdk-build)
  (OpenJDK, GPLv2 with the Classpath Exception; source: [openjdk/jdk](https://github.com/openjdk/jdk)), its licences in the APK's `assets/horizon-jre/legal`
- Inspired by Vivecraft and QuestCraft

Minecraft is a trademark of Mojang Synergies AB. Meta Quest is a trademark of Meta Platforms, Inc.
