#!/bin/sh
# Horizon VR: makes horizon-derived.zip from your own Minecraft: Java Edition.
# Play Minecraft 26.3 once from the official launcher first, then run this file.
cd "$(dirname "$0")" && exec ./horizon-prepare vr-pack
