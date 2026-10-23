# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status

Design only: no code, no build system, no tests yet. `DESIGN.md` is the spec (a draft, written
with RFC 2119 MUST/SHOULD wording); `PARITY.md` lists deliberate deviations from Java Edition.
The next piece of work is `DESIGN.md` §14 step 1 (a Rust app on ctru-rs drawing a textured cube
through citro3d). When a build, test or lint command comes into being, add it here.

## What the project is

A from-scratch, Java Edition–behaving Minecraft for the **New** 3DS / New 2DS XL (the original
3DS is not a target). Behaviour parity with Java is the measure; rendering and UI are the 3DS's
own. Singleplayer first, Java servers later. Rust throughout.

**Build bases**: the player picks any Java release **from 1.13 onward** when building, and the
game behaves as *that* release (worldgen, mechanics, save format, protocol). 1.21.11 is
implemented first, older bases later. **1.12.2 and earlier are unsupported** (the Flattening
reorganised block states, IDs, saves and data), so never add pre-1.13 paths. Version-specific
facts matter: worldgen noise is only data from 1.18.2 (earlier generators must be written in
`core`), Xoroshiro128++ is 1.18+, POI regions 1.14+, entity regions 1.17+, the configuration
protocol phase 1.20.2+, official mappings 1.14.4+ (1.13–1.14.3 are read via unofficial community
mappings; no mappings are ever committed or needed by the build).

The build base is **chosen at compile time**: one binary per base, the build tool passing the
version to `cargo`, version-specific code behind `cfg`/Cargo features. Don't add run-time version
branching. `PARITY.md` entries carry a release range (`1.13–`, `1.13–1.17.1`) saying which build
bases each kept/fixed bug applies to.

## Rules that are easy to break

- **No Mojang code or assets in the repo, ever.** Textures, sounds, models, lang files and data
  packs come from the player's own jar via the build tool, landing in `/assets` (gitignored).
  Built `.3dsx`/`.cia`/`.smdh` are gitignored and never redistributed, since they contain the
  player's converted assets.
- **Reimplement, don't translate.** Java is unobfuscated since 26.1, so vanilla can be read for
  understanding, but our code MUST be our own structure in Rust, never a line-by-line port.
  Where vanilla behaviour is *data* (worldgen noise/density functions, loot, recipes, tags), read
  it from the jar at build time rather than hand-coding it.
- **Exactness over speed.** RNG (`java.util.Random` LCG, Xoroshiro128++, positional random
  factories) must match bit for bit; update/tick ordering must match Java's (redstone depends on
  it); float math follows Java (`float`/`double`, no FMA where Java has none, `StrictMath` where
  Java uses it). A slow tick slows the game, as in Java; it never skips work.
- **Bugs: fixed unless listed in `PARITY.md`.** A Java bug is only kept (e.g. bedrock breaking,
  TNT duping, quasi-connectivity/MC-108) by explicit user decision, recorded in `PARITY.md` with
  its MC- number. Fixed bugs get recorded there too.
- Decisions go in `DESIGN.md` §13 with a date; open questions in §12. Don't settle a §12 question
  silently in code — raise it.

## Planned architecture (`DESIGN.md` §4)

One Cargo workspace:

| Crate | Runs on | Role |
| --- | --- | --- |
| `n3dsr-build` | PC | Microsoft device-code sign-in → Xbox Live → XSTS → MC services; ownership check; download jar per Mojang's version manifest (SHA-1 checked); convert assets into RomFS (texture atlases in PICA200 formats, baked block-model quads, ADPCM/Vorbis audio, data packs compiled to binary tables, density functions compiled to bytecode); then build `.3dsx`/`.cia` |
| `core` | 3DS + PC | The game itself: registries, chunks, world, entities, ticking, worldgen, saves. **Must contain no 3DS-specific code** so it builds and tests on a PC |
| `protocol` | 3DS + PC | Java's packets as types and codecs |
| `server` | 3DS + PC | Integrated server: `core` at 20 TPS, speaking `protocol` |
| `client` | 3DS | citro3d rendering (via FFI or `citro3d` Rust bindings — undecided), input, `ndsp` sound, UI |

Singleplayer = client + integrated server in one process, exchanging `protocol` messages over an
in-memory queue (as Java does). Multiplayer swaps the queue for a socket; nothing else changes —
so client↔server communication must always go through `protocol` types, even in singleplayer.
Worldgen and lighting run as background jobs a chunk at a time, never inside a tick.

Worlds are saved in Java's Anvil format (region `.mca` files, NBT, `level.dat`) under
`/3ds/n3dsr/saves/` so a world folder copies between PC and 3DS unchanged.

## Testing (planned, `DESIGN.md` §11)

`core`/`protocol`/`server` are tested on the PC. Parity tests compare our chunks block-by-block
against a headless vanilla server run from the player's own server jar for chosen seeds, and
replay recorded vanilla redstone/physics traces against `core`. On-device performance is measured
on hardware and in the Azahar emulator.

## Toolchain

devkitPro (devkitARM, libctru, citro3d) plus `ctru-rs` (rust3ds) with std. Target hardware: New
3DS, 804 MHz quad ARM11, app memory 178 MB (extended-memory exheader), PICA200 GPU.

License: GPL-3.0-or-later.
