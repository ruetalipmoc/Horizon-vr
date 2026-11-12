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
> You need to **own Minecraft: Java Edition**. This repository and its releases contain **no Mojang files**.

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

1. Turn on **developer mode** for your Quest 3 (Meta Horizon app on the phone → your headset → Developer mode).
2. Download **`Horizon-VR-v*.apk`** from the [latest release](https://github.com/Boboetto/Horizon-vr/releases/latest) and install it:
   - **SideQuest**: connect the headset and drag the APK onto SideQuest ("Install APK file from folder");
   - or **Meta Quest Developer Hub**: drag the APK onto the device;
   - or in the headset: open the APK from the Quest browser's downloads and install the package (unknown sources allowed).
3. In the headset, open **App Library → Unknown sources → Horizon VR**.

## First start: the game files

The first start downloads the official game files from Mojang (about 525 MB). Then it asks for
**`horizon-derived.zip`**: game data that only Minecraft's own Java code can produce, so you make it on a computer
from your own copy of the game, in one step:

1. On your computer, play **Minecraft: Java Edition 26.3** once from the **official launcher**.
2. Download **`Horizon-VR-Prepare-windows.zip`** (or `-linux.zip`) from the
   [latest release](https://github.com/Boboetto/Horizon-vr/releases/latest) and unzip it.
3. Double-click **`make-horizon-derived.bat`** (Linux: `make-horizon-derived.sh`). After about a minute
   `horizon-derived.zip` is in the same folder. No Java to install: it uses the Minecraft launcher's own.
4. Copy `horizon-derived.zip` to the headset, into `Android/data/dev.horizonlab.horizon/files/`
   (SideQuest's file manager or a USB cable), or pick it with the button on Horizon VR's setup screen.

`horizon-derived.zip` is made from Mojang's game: keep it for yourself and do not share it.

## In italiano

Horizon VR è un APK standalone per **Meta Quest 3**: Minecraft: Java Edition 26.3 in VR, senza PC.
Serve possedere Minecraft Java. Installa l'APK dell'ultima release con **SideQuest** o **Meta Quest Developer Hub**
(modalità sviluppatore attiva), poi aprilo da **Libreria → Origini sconosciute**. Al primo avvio scarica i file
ufficiali da Mojang e chiede `horizon-derived.zip`: crealo sul PC con `Horizon-VR-Prepare` (doppio clic su
`make-horizon-derived.bat`, dopo aver avviato una volta Minecraft 26.3 dal launcher ufficiale) e copialo nel visore.

## Building

```sh
OPENXR_LOADER=/path/to/libopenxr_loader.so ANDROID_HOME=~/android/sdk ./packaging/android/build_apk.sh
```

`libopenxr_loader.so` is the Khronos OpenXR loader for Android (`org.khronos.openxr:openxr_loader_for_android`,
Apache 2.0). The VR code is in `crates/horizon-xr`, the renderer's `render_xr` and
`crates/horizon-client/src/game/android_input.rs`.

## Credits

- **Horizon**, Minecraft: Java Edition rewritten in Rust, by its authors (MIT)
- **Horizon for Android** by [Rayan-XOO](https://github.com/Rayan-XOO/horizon) (MIT), which this project forks
- The Khronos OpenXR loader (Apache 2.0)
- Inspired by Vivecraft and QuestCraft

Minecraft is a trademark of Mojang Synergies AB. Meta Quest is a trademark of Meta Platforms, Inc.
