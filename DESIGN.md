# Minecraft New 3DS Revival: design specification

Status: **draft** (2026-10-22), for review. Nothing is built yet.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as described in
RFC 2119.

## 1. Goals

1.1. **A Minecraft for the New Nintendo 3DS (and New 2DS XL) that behaves like Java Edition**:
the same blocks, items, mobs, crafting, redstone, fluids and world generation (the same world from
the same seed), first as Java 1.21.11, then following Java's releases (26.1 onward). Behaviour is
the measure, not looks: rendering and the interface are the 3DS's own (§7, §8).

1.2. **Bugs players rely on are kept, the others fixed.** Kept: breaking bedrock (piston and TNT
methods), TNT duplication, quasi-connectivity, and whatever else technical players build on; the
list is `PARITY.md`'s, and a bug goes on it only by decision. Every other known bug is fixed, and
the fix noted there too.

1.3. **Singleplayer first** (decided §13): the game and its world run on the 3DS. Java servers
come after (§9), sharing the same core.

1.4. **Java's worlds**: worlds are saved in Java's format (§6), so a world moves between a PC and
the 3DS by copying its folder.

1.5. **Not targeted**: the original 3DS and 2DS (a third of the CPU, half the memory); Bedrock
Edition's servers or worlds; mods.

## 2. What we ship, and what you bring

2.1. **This project ships no Mojang code and no Mojang assets.** Its own code is GPL-3.0-or-later.
Textures, sounds, models, language files and data packs are Mojang's: each player's build tool
takes them from that player's own copy of the game (§3).

2.2. **The build needs a Microsoft account that owns Minecraft: Java Edition.** The build tool
signs in (§3.1) and checks the account's entitlements before it downloads anything; without
ownership it stops.

2.3. **Behaviour is reimplemented, not copied.** Java Edition is unobfuscated since 26.1 (2025),
so how vanilla works can be read. Our code is written from that understanding, in our own
structure and in Rust, never translated line by line from Mojang's. Where vanilla's behaviour is
data (§3.3), we read the data from the player's jar instead of rewriting it.

2.4. Releases are source and the build tool; a built `.3dsx`/`.cia` holds the player's converted
assets and is never redistributed.

## 3. The build tool (`n3dsr-build`, Rust, on the PC)

3.1. **Sign-in**: Microsoft's device-code flow (a code to type at `microsoft.com/link`), then
Xbox Live, XSTS, and Minecraft services, as launchers do; the entitlement check (§2.2) reads the
account's licenses. Tokens are kept in the user's config directory, refreshed, never logged.

3.2. **Download**: the chosen version's entry in Mojang's version manifest, its client jar (and,
for the data, the server jar where it differs), checked against the manifest's SHA-1s, cached.

3.3. **Convert**, everything the 3DS needs from the jar, into the app's RomFS:
- textures: packed into atlases and converted to the 3DS GPU's formats (§7.3), mipmapped;
- block models and blockstates: baked into per-state quad lists (the work Java does at resource
  load, done once on the PC);
- sounds: decoded and re-encoded for the 3DS's DSP (IMA-ADPCM, or kept Vorbis where streamed,
  §8.3);
- data packs (world generation's noise settings, density functions, biomes, structures, features;
  loot tables; recipes; tags; advancements): compiled into compact binary tables, and density
  functions into a small bytecode the core interprets (§5.4);
- registries and language files.

3.4. **Build**: the converted assets, then the app (`cargo` for the 3DS target, §10), into a
`.3dsx` (homebrew launcher) and a `.cia`.

## 4. Architecture

4.1. One Rust workspace:

| Crate | Runs on | Holds |
| --- | --- | --- |
| `n3dsr-build` | PC | §3 |
| `core` | 3DS and PC | the game: registries, blocks, chunks, the world, entities, ticking, world generation, saves. No 3DS code: it builds and tests on a PC (§11). |
| `protocol` | 3DS and PC | Java's packets, as types and codecs |
| `server` | 3DS and PC | the integrated server: `core` driven at 20 ticks a second, speaking `protocol` |
| `client` | 3DS | rendering, input, sound, the interface; speaks `protocol` |

4.2. **Singleplayer is a client and an integrated server in one process**, as in Java: the
server's thread runs the world, and client and server exchange `protocol`'s messages through an
in-memory queue. Joining a Java server (§9) swaps the queue for a socket; nothing else changes.

4.3. The server runs on the cores the client doesn't (§4.4). World generation and lighting run as
jobs in the background, a chunk at a time, never inside a tick.

4.4. **The hardware** (New 3DS): an 804 MHz quad-core ARM11 MPCore, 256 MB of FCRAM, the PICA200
GPU. An app gets 124 MB of memory by default, or 178 MB if its extended header asks for it (which
turns off the system browser and Miiverse overlay): we ask for 178 MB. Which cores an app may run
threads on, and how much of the system core it can have, is open (§12).

## 5. The simulation

5.1. **20 ticks a second, as Java**, within the simulation distance (default 4 chunks, a setting;
Java's minimum is 5). When a tick runs long the game slows, as Java's does, rather than skipping
work: behaviour is exact before it is fast.

5.2. **Exact randomness**: `java.util.Random`'s LCG, Xoroshiro128++ and their seeding (positional
random factories included) reimplemented bit for bit, since world generation and many behaviours
depend on their exact sequence.

5.3. **Exact order**: block updates, neighbour updates, scheduled ticks, random ticks, block
events and entity ticking run in Java's order, which redstone depends on (quasi-connectivity
included, §1.2).

5.4. **World generation from the data** (§3.3): noise router, density functions (as bytecode),
surface rules, carvers, features and structures, by Java's chunk stages (structure starts,
references, biomes, noise, surface, carvers, features, light). The same seed MUST give the same
blocks as Java (§11.2).

5.5. Floating point: Java's `float`/`double` semantics (IEEE 754, no fused multiply-add where Java
has none), and `StrictMath` where Java uses it, so physics and generation round as Java's do. The
3DS's VFP has double precision.

## 6. Worlds

6.1. **Java's format**: a world folder of `level.dat`, region files (`.mca`, Anvil: chunks as
zlib-compressed NBT, sectors of 4 KiB), entity and POI regions, player data; the data version of
the release played. On the SD card under `/3ds/n3dsr/saves/`.

6.2. A world from a newer Java release is refused, not opened (as Java does); an older one is
upgraded by the same data fixes Java applies, as far as we implement them, and otherwise refused.

## 7. Rendering (`client`)

7.1. citro3d (devkitPro's PICA200 library), through FFI from Rust (§10).

7.2. Chunk sections (16³) meshed on a background job when they change: faces against opaque
neighbours culled, per-vertex light and ambient occlusion (Java's smooth lighting), opaque,
cutout and translucent passes, translucent faces sorted. Render distance separate from simulation
distance (§5.1).

7.3. Textures in the GPU's formats (ETC1 where it looks right, RGBA4/RGB565 or RGBA8 where it
doesn't), atlases within the GPU's size limits (§12); animated textures by updating atlas tiles.

7.4. Stereoscopic 3D from the slider. The bottom screen holds the hotbar, inventory, crafting,
chat and the map, touch-driven.

## 8. Input, interface, sound

8.1. Circle pad to move (or the D-pad: a setting, for a worn or broken pad), C-stick to look, ZL
use, ZR attack, L/R hotbar, A jump, B sneak, X inventory, Y drop; all rebindable.

8.2. Menus are the 3DS's own layout, with Java's options and semantics behind them.

8.3. Sound through the DSP (`ndsp`): effects as ADPCM in memory, music streamed from the SD card.

## 9. Java servers (after singleplayer)

9.1. One protocol version at a time, the release played (1.21.11: protocol 774, to confirm).
Handshake, status, login, configuration (registries from the server), play.

9.2. **Sign-in on the 3DS**: the device-code flow on the 3DS itself (the code shown on screen,
typed in on a phone or PC), tokens kept on the SD card and refreshed (decided §13). Online-mode
servers' encryption (RSA, then AES/CFB8) and compression (zlib), via mbedTLS.

9.3. **Chat signing off by default**, as the No Chat Reports mod: messages are sent unsigned, and
signed only to a server that requires it (`enforce-secure-profile`), which the player is asked to
allow once per server. **No telemetry**: nothing is ever sent to Mojang or Microsoft but sign-in
and session joining; what the client tells a server (view distance, language, skin parts, server
listing) is the player's settings, each one off-able.

9.4. Not aimed for: the large networks' anti-cheat (Hypixel and the like).

## 10. Language and toolchain

10.1. Rust throughout: `ctru-rs` (the rust3ds project's safe wrapper over libctru) on the 3DS, with
std. The GPU through citro3d: by its Rust bindings if they cover what §7 needs (§12), else a thin
C layer over citro3d that the Rust client calls.

10.2. devkitPro's devkitARM, libctru and citro3d are build dependencies, as is the player's copy
of Minecraft (§2.2).

## 11. Testing

11.1. `core`, `protocol` and `server` build and test on the PC; nothing in them knows the 3DS.

11.2. **Parity tests against vanilla**: for chosen seeds, chunks generated by a vanilla server
(headless, from the player's own server jar) are compared block by block with ours; redstone and
physics cases are recorded from vanilla (a contraption's states tick by tick, an entity's
positions) and replayed against `core`.

11.3. On the 3DS: frame time, tick time and memory, measured on hardware and in the Azahar
emulator, per step.

## 12. Open questions

- Which of the New 3DS's four cores an app may use, and at what share (the system core's limit).
- The PICA200's texture size and format limits as citro3d exposes them, and how much VRAM the
  chunk meshes may take.
- How complete the Rust citro3d bindings are (§10.1).
- Whether 1.18+ world generation can run fast enough to keep ahead of walking (and flying) speed
  at a small render distance, and what to do when it can't.
- How far back worlds are upgraded (§6.2).
- Which releases after 1.21.11 to follow, and how quickly.

## 13. Decisions

- 2026-10-22 (user): a separate repository, `MinecraftNew3DSRevival`; exact Java parity in
  behaviour, keeping the bugs players rely on (breaking bedrock, TNT duping, quasi-connectivity);
  released as source with a build tool that needs a real copy of Minecraft and an account that
  owns it; Rust (or C++) over ctru; Java server compatibility, the big networks not aimed for;
  singleplayer first; sign-in by device code on the 3DS; No Chat Reports-style chat signing and
  control over what is sent; GPL-3.0.
- 2026-10-22: GPL-3.0-or-later (the FSF's recommended form; `-only` if preferred); the original 3DS
  not targeted (§1.5); worlds in Java's format (§6).

## 14. Steps

Each step its own commit, its tests passing.

1. **Toolchain**: a Rust app on ctru-rs drawing a textured cube through citro3d, on hardware and
   Azahar; the decision §10.1 leaves open.
2. **The build tool**: sign-in, ownership, download, textures and models converted.
3. **A Java world on screen**: a world copied from a PC, its region files read (§6) and rendered,
   flown through; no simulation yet.
4. **Movement**: the player's physics (collision, gravity, sprinting, swimming), checked against
   recorded vanilla traces.
5. **Blocks and items**: breaking, placing, the inventory, crafting from the recipes.
6. **Ticking**: block updates, redstone, fluids, random ticks, in Java's order; the parity cases.
7. **World generation**: superflat, then noise; seed parity tests (§11.2).
8. **Entities and mobs**: AI, spawning, combat, the loot tables.
9. **Saving** (§6), so a world goes back to the PC.
10. **Java servers** (§9).
