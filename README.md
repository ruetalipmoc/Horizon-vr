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

1. Turn on **developer mode** for your Quest 3: in the **Meta Horizon** app on your phone → Menu → Devices →
   your headset → Headset settings → Developer mode (Meta asks you to create a free developer account once).
2. Download **`Horizon-VR-v*.apk`** from the [latest release](https://github.com/Boboetto/Horizon-vr/releases/latest).
3. Install it with **SideQuest**: connect the headset to the computer with its USB-C cable, accept "Allow USB debugging"
   in the headset, then drag the APK onto the SideQuest window.
   (Or drag it onto **Meta Quest Developer Hub**.)
4. In the headset, open **App Library → Unknown sources → Horizon VR**.

## First start: horizon-derived.zip, from your computer to the Quest 3

On its first start Horizon VR needs **`horizon-derived.zip`**: game data that only Minecraft's own Java code can make,
so your computer makes it from your own copy of the game, then sends it to the headset over Wi-Fi.
The computer and the Quest 3 must be on the **same Wi-Fi network**.

**On the computer**

1. Play **Minecraft: Java Edition 26.3** once from the **official Minecraft launcher**, then close it.
2. Download **`Horizon-VR-Prepare-windows.zip`** (Linux: `Horizon-VR-Prepare-linux.zip`) from the
   [latest release](https://github.com/Boboetto/Horizon-vr/releases/latest) and unzip it (right click → Extract all).
3. Open the unzipped folder and double-click **`make-horizon-derived.bat`** (Linux: `make-horizon-derived.sh`).
   - If Windows shows **"Windows protected your PC"**: click **More info**, then **Run anyway**.
   - If Windows asks about the **firewall**: tick **Private networks** and click **Allow access**.
     Without this the headset cannot find the computer.
4. Wait about a minute. When the window says **"Now put on the headset"**, leave it **open**.

**On the Quest 3**

5. Open **Horizon VR**. On the setup screen press **"Get it from my computer (Wi-Fi)"**.
6. When it says **"Got horizon-derived.zip ✓"**, press **"Download game files"**: it downloads the official
   game files from Mojang (about 700 MB, a few minutes on Wi-Fi), then the game starts.
7. You can now close the window on the computer.

**If the headset cannot find the computer** ("no computer answered"):

- check that the window on the computer is still open and says "Now put on the headset";
- check that the Quest 3 and the computer are on the same Wi-Fi (not a guest network);
- on Windows, allow the program in the firewall: Start → "Allow an app through Windows Firewall" →
  tick `horizon-prepare` for **Private**, then press the button in the headset again.

**Or with the cable**: connect the Quest 3 to the computer with USB, open SideQuest, click the **folder icon**
(top right), go to `Android/data/dev.horizonlab.horizon/files/` and drag `horizon-derived.zip` there.
Then open Horizon VR and press **"Download game files"**.

`horizon-derived.zip` is made from Mojang's game: keep it for yourself and do not share it.

## In italiano

1. **Visore:** attiva la modalità sviluppatore (app Meta Horizon sul telefono → Dispositivi → visore →
   Impostazioni → Modalità sviluppatore) e installa l'APK dell'ultima release trascinandolo su **SideQuest**.
   Si apre da **Libreria → Origini sconosciute → Horizon VR**.
2. **Computer:** avvia una volta Minecraft: Java Edition 26.3 dal launcher ufficiale. Scarica ed estrai
   `Horizon-VR-Prepare-windows.zip` e fai doppio clic su `make-horizon-derived.bat`.
   Se Windows dice "PC protetto": *Ulteriori informazioni → Esegui comunque*. Se chiede del firewall:
   spunta *Reti private → Consenti accesso*. Dopo circa un minuto la finestra dice "Now put on the headset":
   **lasciala aperta**.
3. **Quest 3** (stesso Wi-Fi del computer): apri Horizon VR, premi **"Get it from my computer (Wi-Fi)"**,
   poi **"Download game files"**. Finito, puoi chiudere la finestra sul computer.
4. Se il visore non trova il computer: controlla che la finestra sia aperta, che siate sullo stesso Wi-Fi e
   che il firewall di Windows consenta `horizon-prepare` sulle reti private. In alternativa, col cavo: in
   SideQuest icona cartella → `Android/data/dev.horizonlab.horizon/files/` → trascina lì `horizon-derived.zip`.

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
