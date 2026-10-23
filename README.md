# Minecraft New 3DS Revival

A from-scratch Minecraft for the New Nintendo 3DS that behaves like Java Edition: the same blocks,
mobs, redstone and world generation as the Java release you build it from, with Java's worlds and,
later, Java servers. Any release from 1.13 onward can be a build base (1.21.11 comes first); 1.12.2
and earlier are not supported, since 1.13's Flattening reorganised blocks, items, saves and data. Bugs players depend on (breaking bedrock, TNT duping, quasi-connectivity) stay;
the rest are fixed.

**Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.**

This repository holds none of Mojang's code or assets. The build tool signs in to your Microsoft
account, checks that it owns Minecraft: Java Edition, downloads that version's client from Mojang
as the official launcher does, and converts its textures, sounds and data for the 3DS. What it
makes is for your own console.

Status: design (`DESIGN.md`). License: GPL-3.0-or-later (`LICENSE`).
