# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Status

Early. `DESIGN.md` §14 step 1 is done: `n3dsr-client` draws a textured cube in stereo and prints
a hardware probe (cores, memory) on the bottom screen. `n3dsr-core` has `JavaRandom`
(`java.util.Random`, bit-exact). `DESIGN.md` is the spec (a draft, RFC 2119 MUST/SHOULD
wording); `PARITY.md` lists deliberate deviations from Java Edition.

## Commands

```sh
cargo test                                   # all PC crates (the client is excluded by default-members)
cargo test -p n3dsr-core --test java_random  # one test file; add a test name to filter further
cargo clippy --all-targets && cargo fmt --check
cargo test --no-default-features --features mc1_X_Y   # another build base, once it exists
java tools/vectors/JavaRandomVectors.java > crates/core/tests/data/java_random.txt  # regenerate vectors
```

The 3DS client needs devkitPro (`3ds-dev`) and `cargo-3ds`. devkitPro's env script doesn't put
the ARM linker on `PATH`, so:

```sh
source /etc/profile.d/devkit-env.sh && export PATH=$DEVKITARM/bin:$PATH
cargo 3ds build -p n3dsr-client --release    # target/armv6k-nintendo-3ds/release/n3dsr-client.3dsx
flatpak run org.azahar_emu.Azahar "$PWD/target/armv6k-nintendo-3ds/release/n3dsr-client.3dsx"
curl -u USER:PASS -T target/armv6k-nintendo-3ds/release/n3dsr-client.3dsx ftp://3DS_IP:5000/3ds/
```

The user tests on a real New 3DS (Luma3DS) by FTP to ftpd, then the Homebrew Launcher; check the
target path is free before uploading. Trust hardware over Azahar for measurements (`DESIGN.md`
§11.3). The toolchain is pinned in `rust-toolchain.toml`; don't bump it casually (§10.3).

Test vectors come from running the real thing (the JDK, or later a vanilla jar) through small
Java programs in `tools/vectors/`; their output is committed under `crates/*/tests/data/`, and
the Rust tests replay it. Floats are compared as raw bits.

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
protocol phase 1.20.2+, official mappings 1.14.4+ (1.13–1.14.3 are read via community mappings:
Feather for 1.13.x, Yarn for 1.14–1.14.3; only CC0/MIT/BSD/GPL/MPL-licensed mappings may be
used, so never MCP; no mappings are ever committed or needed by the build).

The build base is **chosen at compile time**: one binary per base, the build tool passing the
version to `cargo`, version-specific code behind `cfg`. **One Cargo feature per release**, never
per range: behaviour spanning releases is gated with `any(...)` over each release's feature.
Features are spelled `mc1_21_11`. Exactly one release feature per build: `crates/core/build.rs`
fails the build otherwise, and `mc1_21_11` is the default feature in each crate (another base
needs `--no-default-features`). Crates above `core` forward their release features down. Don't
add run-time version branching. `PARITY.md` entries carry a release range (`1.13–`,
`1.13–1.17.1`) saying which build bases each kept/fixed bug applies to.

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

## Architecture (`DESIGN.md` §4)

One Cargo workspace, crates under `crates/<short>/`, packages named `n3dsr-<short>` (a package
can't be called `core` without clashing with Rust's `core`):

| Crate | Runs on | Role |
| --- | --- | --- |
| `n3dsr-export` (planned, `mod/`) | PC, in Minecraft | Fabric-family mod (Fabric 1.14+, Legacy Fabric 1.13.2, Ornithe 1.13/1.13.1), built per release with Stonecutter; Rust compiled to JVM bytecode via `rustc_codegen_jvm` where possible, Java only for entrypoints/mixins (pending the step 2a spike). Checks ownership with Mojang via the game's session, then exports what the game loaded (resource packs applied). Triggered from Mod Menu's config screen, `/n3dsr export`, or a launch argument |
| `n3dsr-build` | PC | Run by the player on an export folder (no sign-in, no downloading); convert assets into RomFS (texture atlases in PICA200 formats, baked block-model quads, ADPCM/Vorbis audio, data packs compiled to binary tables, density functions compiled to bytecode); then build `.3dsx`/`.cia` |
| `n3dsr-core` | 3DS + PC | The game itself: registries, chunks, world, entities, ticking, worldgen, saves. **Must contain no 3DS-specific code** so it builds and tests on a PC |
| `n3dsr-protocol` | 3DS + PC | Java's packets as types and codecs |
| `n3dsr-server` | 3DS + PC | Integrated server: `core` at 20 TPS, speaking `protocol` |
| `n3dsr-client` | 3DS | Rendering through the `citro3d` Rust bindings (raw `citro3d-sys` for depth/alpha/blend state they don't wrap), input, `ndsp` sound, UI. Threads that must sit on a given core use libctru's `threadCreate`, since `std::thread` can't choose one; app threads go on cores 2 and 3 (`DESIGN.md` §4.4.1) |

Singleplayer = client + integrated server in one process, exchanging `protocol` messages over an
in-memory queue (as Java does). Multiplayer swaps the queue for a socket; nothing else changes —
so client↔server communication must always go through `protocol` types, even in singleplayer.
Worldgen and lighting run as background jobs a chunk at a time, never inside a tick.

Worlds are saved in Java's Anvil format (region `.mca` files, NBT, `level.dat`) under
`/3ds/n3dsr/saves/` so a world folder copies between PC and 3DS unchanged.

## Testing (`DESIGN.md` §11)

`core`/`protocol`/`server` are tested on the PC. Parity tests compare our chunks block-by-block
against a headless vanilla server run from the player's own server jar for chosen seeds, and
replay recorded vanilla redstone/physics traces against `core`. On-device performance is measured
on hardware and in the Azahar emulator.

## Toolchain

devkitPro (devkitARM, libctru, citro3d) plus `ctru-rs` (rust3ds) with std. Target hardware: New
3DS, 804 MHz quad ARM11, app memory 178 MB (extended-memory exheader), PICA200 GPU.

License: GPL-3.0-or-later.
