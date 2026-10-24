# Minecraft New 3DS Revival: design specification

Status: **draft** (2026-10-22), for review. Nothing is built yet.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as described in
RFC 2119.

## 1. Goals

1.1. **A Minecraft for the New Nintendo 3DS (and New 2DS XL) that behaves like Java Edition**:
the same blocks, items, mobs, crafting, redstone, fluids and world generation (the same world from
the same seed). Behaviour is the measure, not looks: rendering and the interface are the 3DS's own
(§7, §8).

1.1.1. **Build bases: any Java release from 1.13 onward.** The player picks a release when building
(§3), and the game behaves as that release: its blocks, its world generation, its saves, its
protocol. 1.21.11 comes first; older releases are added after it, and new ones as Java publishes
them (26.1 onward). 1.12.2 and earlier are not supported: 1.13 ("the Flattening", data version
1519) reorganised block states, item IDs, the save format and the data layout, and everything here
is built on the post-1.13 structure.

1.2. **Bugs players rely on are kept, the others fixed.** Kept: breaking bedrock (piston and TNT
methods), TNT duplication, quasi-connectivity, and whatever else technical players build on; the
list is `PARITY.md`'s, and a bug goes on it only by decision. Every other known bug is fixed, and
the fix noted there too.

1.3. **Singleplayer first** (decided §13): the game and its world run on the 3DS. Java servers
come after (§9), sharing the same core.

1.4. **Java's worlds**: worlds are saved in Java's format (§6), so a world moves between a PC and
the 3DS by copying its folder.

1.5. **Not targeted**: the original 3DS and 2DS (a third of the CPU, half the memory); Java 1.12.2
and earlier (§1.1.1); Bedrock Edition's servers or worlds; mods.

## 2. What we ship, and what you bring

2.1. **This project ships no Mojang code and no Mojang assets.** Its own code is GPL-3.0-or-later.
Textures, sounds, models, language files and data packs are Mojang's: each player's build reads
them from their own installed copy of the game, once a mod of ours has confirmed they own it (§3).

2.2. **The build needs a game signed in to an account that owns Minecraft: Java Edition.** The
mod (§3.1) runs inside the player's own game, launched by their own launcher, and checks
ownership exactly as an online-mode server does: the game's own code sends Mojang's session
server a `join`, and the public `hasJoined` endpoint confirms the account has a Java Edition
profile. The access token is read from the game only to hand straight back to vanilla's
`joinServer`, as vanilla's own login does, and is never stored, logged or sent anywhere else; no
API is called that vanilla doesn't call. An offline or cracked launch can't pass (decided §13). Signing in from a tool of
our own was dropped: Mojang serves its API only to Azure apps it has approved, by a review with
no stated timeline (§12). The proof the mod writes is a checkpoint, not DRM: it can be forged by
hand, and isn't meant to stop someone determined to.

2.3. **Behaviour is reimplemented, not copied.** Java Edition is unobfuscated since 26.1 (2025),
and Mojang has published official mappings since 1.14.4, so how vanilla works can be read for
nearly every build base. 1.13, 1.13.1, 1.13.2, 1.14, 1.14.1, 1.14.2 and 1.14.3 have no official
mappings and are read through community-made ones, **only under CC0, MIT, BSD, GPL or MPL**
(decided §13; MCP's terms rule it out): FabricMC's Yarn (CC0) for 1.14 to 1.14.3, and OrnitheMC's
Feather (CC0) for 1.13 to 1.13.2, which Yarn does not cover. Mappings of any kind are only for
reading: none are committed, shipped or needed by the build. Our code is written from that understanding, in our own
structure and in Rust, never translated line by line from Mojang's. Where vanilla's behaviour is
data (§3.3), we read the data from the player's jar instead of rewriting it.

2.4. Releases are source and the build tool; a built `.3dsx`/`.cia` holds the player's converted
assets and is never redistributed.

## 3. Export and build

Two parts (decided §13): a mod that confirms ownership from inside the running game, and a
converter the player runs afterwards on the game files the player's launcher installed.

3.1. **The mod (`n3dsr-export`)**, one build per release, on the Fabric Loader family: Fabric for
1.14 onward, Legacy Fabric for 1.13.2, Ornithe for 1.13 and 1.13.1 (neither of the others reaches
them). It does only what proving ownership needs (decided §13):
- it shows in Mod Menu (and the like), and opening its config screen starts the build setup; it
  also runs on a client command (`/n3dsr export`), and on a launch argument, at the title screen
  and then quitting, for scripts and for building many releases in a row;
- it checks ownership (§2.2);
- it writes a proof file, `<config dir>/n3dsr/proofs/<release>.json` (decided §13; the config
  dir is the one Rust's `dirs` crate gives, `~/.config` on Linux, so `n3dsr-build` finds it): the
  account's name and UUID, the release, the time, and the paths and SHA-1s of the client jar and
  asset index the game is running from, and where the asset objects are. Then it tells the
  player the `n3dsr-build` command to run next. The client jar is the one on the classpath with
  `net/minecraft/client/main/Main.class` (never obfuscated; Fabric's remapped copy isn't on the
  classpath); the asset folders are the ones the game was started with, kept by a mixin on
  `Minecraft`'s constructor, since the game doesn't keep them.

3.1.1. **The mod is Rust** (decided §13): compiled to JVM bytecode by `rustc_codegen_jvm`, which
can call Java classes, so the Rust calls the game's own classes directly. Java only where Fabric
requires a JVM class of its own shape: entrypoints and mixins. Stonecutter builds the mod per
release and switches its Java between releases; the Rust uses the same per-release Cargo
features as `core` (§5.6). Before 26.1 the game's classes run under Fabric's intermediary names,
and the Rust's classes go into the mod's jar before Loom's `remapJar`, which remaps them with the
Java's. Proven by step 2a (2026-10-23): Rust bound to `BuiltInRegistries.BLOCK` and
`DefaultedRegistry.size()` came out as `class_7923.field_41175` and `class_7922.method_10204`
(`size` resolved through `IdMap`, where it's declared), and counted 1166 blocks in a 1.21.11
client. The Rust compiles to Java 8 class files, so the same holds for the oldest releases.
Step 2b (2026-10-23) ran the whole check in the user's Prism instance: Mojang confirmed the
account, and the proof's client jar and asset index SHA-1s matched Mojang's published ones for
1.21.11 (`ba2df812…`, index 29 `a2f0558b…`).

3.1.2. Two costs found by the spike, for later: `cargo jvm package` bundles the whole Rust
standard library (a 17 MB jar of some 14,000 classes, most unused); and its runtime lives in
`org.rustlang.*`, which would clash with another mod built the same way unless relocated (§12).

3.2. **The converter (`n3dsr-build`, Rust, on the PC)** takes a proof file, checks the jar and
asset index against the SHA-1s in it, and reads the game's files from where the player's launcher
installed them: the client jar, and the asset objects (sounds, and all language files but
`en_us`, which aren't in the jar). It downloads nothing. Since it parses the jar itself, it parses
each release's formats itself.

3.3. **Convert**, everything the 3DS needs from those files, into the app's RomFS:
- textures: packed into atlases and converted to the 3DS GPU's formats (§7.3), mipmapped;
- block models and blockstates: baked into per-state quad lists (the work Java does at resource
  load, done once on the PC);
- sounds: decoded and re-encoded for the 3DS's DSP (IMA-ADPCM, or kept Vorbis where streamed,
  §8.3);
- data packs (world generation's noise settings, density functions, biomes, structures, features;
  loot tables; recipes; tags; advancements): compiled into compact binary tables, and density
  functions into a small bytecode the core interprets (§5.4). Only from 1.18.2 is world
  generation's noise data in the jar; 1.16.2 to 1.18.1 have biomes, features and noise settings as
  data but the noise itself in code, and 1.13 to 1.16.1 have it all in code. For those, `core`
  carries the generator itself (§5.4);
- registries and language files.

3.4. **Build**: the converted assets, then the app (`cargo` for the 3DS target, §10), into a
`.3dsx` (homebrew launcher) and a `.cia`.

3.4.1. **The app's icon is end stone** (decided §13), from the player's own jar
(`assets/minecraft/textures/block/end_stone.png`, the same path in every release from 1.13): the
48×48 icon by nearest-neighbour ×3, the 24×24 by averaging the 48×48's 2×2 blocks. `n3dsr-build`
makes both, since `smdhtool`'s own small icon averages each block's top-left and bottom-right
pixels twice and the other two never. Builds without a proof keep libctru's default icon.

3.5. **Resource packs, as Java takes them** (decided §13): a zip or folder with `pack.mcmeta` and
`assets/`, stacked over the vanilla jar, each pack overriding the ones below it, with Java's pack
format check (`major.minor` from 1.21.x), overlays (per-format subfolders, 1.20.2 onward) and
filters (`filter.block`, hiding lower packs' files). At build time the stack is the game's own:
`options.txt`'s `resourcePacks`, lowest priority first (`vanilla`, built-ins such as
`programmer_art`, and `file/<name>` from `resourcepacks/`; packs that mods provide are left out,
with a warning), so the proof records the game directory (proof format 2). Packs can also be
dropped on the SD card and chosen in the game, converted on the 3DS when chosen. The pack stack
and all the texture conversion below therefore live in a crate both run, `n3dsr-assets` (3DS and
PC, pure Rust: `png`, `zip`), with `n3dsr-build` a front end to it.

3.6. **Textures** (decided §13): everything, in step 2c.
- Atlases are built from the stack's `atlases/*.json` (1.19.3 onward; earlier releases' atlas
  contents are code, per release): `directory`, `single`, `paletted_permutations` (armor trims),
  `unstitch` and `filter` sources. 1.21.11 has 15 atlases; the block atlas is some 1,650 tiles of
  16×16 with every animation frame, within one 1024×1024 page (the PICA200's largest texture).
  Textures outside atlases (entities, environment, fonts and the like) become textures of their
  own, padded to the GPU's power-of-two sizes.
- Formats: sprites with any transparency go to RGBA4 pages, opaque ones to RGB565 pages, so an
  atlas can be two pages.
- Mipmaps as the release's Java makes them. 1.21.11's: a strategy per texture (`texture`
  section's `mipmap_strategy`: `mean`, `cutout`, `strict_cutout`, `dark_cutout`; `auto` is cutout
  if any pixel is transparent), the 2×2 mean in linear light through Java's 10-bit tables, alpha
  rescaled per level to keep the level-0 alpha-test coverage (`alpha_cutoff_bias`), and cutout
  textures' transparent pixels first filled from their nearest opaque ones (`solidify`, or a
  darkened colour for `dark_cutout`). Levels up to the game's Mipmap Levels option (0–4).
- Animations: every frame gets its own tile, and faces switch frames by their UVs on Java's
  timing (`frametime`, `frames`). Interpolated animations (23 in 1.21.11) blend on the GPU: their
  faces carry the current and next frames' UVs and the texture combiner mixes them by the tick's
  progress, as Java does each tick.
- Each page's PNG preview is written beside it (as packed, and decoded back from its GPU format),
  for looking at.

## 4. Architecture

4.1. One Rust workspace:

| Crate | Runs on | Holds |
| --- | --- | --- |
| `n3dsr-build` | PC | §3.2–3.4 |
| `n3dsr-core` | 3DS and PC | the game: registries, blocks, chunks, the world, entities, ticking, world generation, saves. No 3DS code: it builds and tests on a PC (§11). |
| `n3dsr-protocol` | 3DS and PC | Java's packets, as types and codecs |
| `n3dsr-server` | 3DS and PC | the integrated server: `core` driven at 20 ticks a second, speaking `protocol` |
| `n3dsr-client` | 3DS | rendering, input, sound, the interface; speaks `protocol` |

Beside the workspace, in `mod/`: the ownership mod, `n3dsr-export` (§3.1), a Gradle build of its
Java and of a Rust crate compiled for the JVM by its own pinned toolchain (§10.4).

Packages are prefixed `n3dsr-` (a crate named `core` would clash with Rust's own); the text below
uses the short names.

4.2. **Singleplayer is a client and an integrated server in one process**, as in Java: the
server's thread runs the world, and client and server exchange `protocol`'s messages through an
in-memory queue. Joining a Java server (§9) swaps the queue for a socket; nothing else changes.

4.3. The server runs on the cores the client doesn't (§4.4). World generation and lighting run as
jobs in the background, a chunk at a time, never inside a tick.

4.4. **The hardware** (New 3DS): an 804 MHz quad-core ARM11 MPCore, 256 MB of FCRAM, the PICA200
GPU. An app gets 124 MB of memory by default, or 178 MB if its extended header asks for it (which
turns off the system browser and Miiverse overlay): we ask for 178 MB. That takes an extended
header, so a `.cia`; a `.3dsx` gets the 124 MB of whatever title the Homebrew Launcher runs in
(measured, §14 step 1).

4.4.1. **Cores, measured on a New 3DS** (2026-10-23, Luma3DS, `.3dsx`, 804 MHz on): threads created
on cores 0, 2 and 3 ran there, each at the same speed (a 2,000,000-iteration loop in 22.4, 22.4
and 23.3 ms). A thread created on core 1 (the system core, with 30% of it granted first) ran on
core 2 instead, at full speed; why is open (§12). So the server, world generation and lighting
have cores 2 and 3 to themselves, the client keeping core 0. Rust's `std::thread` can't choose a
core on the 3DS: such threads are created through libctru's `threadCreate`.

## 5. The simulation

5.1. **20 ticks a second, as Java**, within the simulation distance (default 4 chunks, a setting;
Java's minimum is 5). When a tick runs long the game slows, as Java's does, rather than skipping
work: behaviour is exact before it is fast.

5.2. **Exact randomness**: `java.util.Random`'s LCG, Xoroshiro128++ (1.18 onward) and their seeding (positional
random factories included) reimplemented bit for bit, since world generation and many behaviours
depend on their exact sequence.

5.3. **Exact order**: block updates, neighbour updates, scheduled ticks, random ticks, block
events and entity ticking run in Java's order, which redstone depends on (quasi-connectivity
included, §1.2).

5.4. **World generation from the data** (§3.3): noise router, density functions (as bytecode),
surface rules, carvers, features and structures, by Java's chunk stages (structure starts,
references, biomes, noise, surface, carvers, features, light). The same seed MUST give the same
blocks as Java of the build base (§11.2). Before 1.18.2 the generator is code, not data (§3.3):
`core` reimplements each era's (1.13–1.17, 1.18–1.18.1) as its own generator.

5.6. **Per-version behaviour**: where releases differ (world generation, mechanics, the bugs of
§1.2), `core` follows the build base, **chosen at compile time** (decided §13): one binary per
build base, the build tool passing the version to `cargo`, and version-specific code selected with
`cfg`, so the 3DS binary carries only its own release's behaviour. **One Cargo feature per
release** (decided §13), never per range: a behaviour spanning releases is gated on every feature
in its span (`any(...)`), not on a range alias, so what a build contains is always readable from
the release alone. Exactly one release feature is on per build; the build fails otherwise.
Features are spelled `mc1_21_11` (decided §13; dots aren't allowed in feature names). Tests run per
build base (§11.2).

5.5. Floating point: Java's `float`/`double` semantics (IEEE 754, no fused multiply-add where Java
has none), and `StrictMath` where Java uses it, so physics and generation round as Java's do. The
3DS's VFP has double precision.

## 6. Worlds

6.1. **Java's format**, as the build base writes it: a world folder of `level.dat`, region files
(`.mca`, Anvil: chunks as zlib-compressed NBT, sectors of 4 KiB), POI regions (1.14 onward),
entity regions (1.17 onward), player data; the data version of the build base. On the SD card under `/3ds/n3dsr/saves/`.

6.2. A world from a release newer than the build base is refused, not opened (as Java does); an older one is
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

9.1. One protocol version at a time: the build base's (1.21.11: protocol 774, to confirm; 1.13:
393). Handshake, status, login, configuration (1.20.2 onward; registries from the server), play.

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
std. The GPU through citro3d's Rust bindings, `citro3d` (decided §13): they cover render targets
with depth, stereo projection, shaders (assembled by `picasso` at compile time), vertex and index
buffers, textures and texture combiners. What they don't wrap yet (setting the depth test, alpha
test and blending, which the passes of §7.2 need; fog) is called through `citro3d-sys` directly.
None of rust3ds's crates are on crates.io: they come from Git, Cargo.lock pinning the commit.

10.3. The toolchain is a pinned nightly (`rust-toolchain.toml`), since the 3DS target builds its
standard library from source. Moving it is deliberate: rust3ds's on-device test harness tracks
rustc's unstable test internals, which changed twice between June and September 2026.

10.2. devkitPro's devkitARM, libctru and citro3d are build dependencies, as is the player's copy
of Minecraft (§2.2).

10.4. The export mod: Gradle (9.7.1, by its wrapper) with Fabric Loom (Legacy Fabric's and
Ornithe's for their releases) and Stonecutter. Gradle itself runs on JDK 25 or 26: Loom 1.18 needs
25, and Gradle 9.7.1 can't read JDK 27's class files. The mod is compiled for the JDK each release
needs (8 for 1.13 to 1.16, 16 for 1.17, 17 for 1.18 to 1.20.4, 21 from 1.20.5, and what 26.x
asks). `rustc_codegen_jvm` (`cargo-jvm`) with the nightly it pins, separate from the 3DS's
(§10.3); `cargo-jvm` installed from the backend's own checkout, and `rcj` (its interop macros)
taken from Git at the backend's commit, since crates.io's copies of both lag the backend that
`cargo jvm setup` clones.

## 11. Testing

11.1. `core`, `protocol` and `server` build and test on the PC; nothing in them knows the 3DS.

11.2. **Parity tests against vanilla**, per supported build base: for chosen seeds, chunks generated by a vanilla server
(headless, from the player's own server jar) are compared block by block with ours; redstone and
physics cases are recorded from vanilla (a contraption's states tick by tick, an entity's
positions) and replayed against `core`.

11.3. On the 3DS: frame time, tick time and memory, measured on hardware and in the Azahar
emulator, per step. Hardware is the authority: Azahar reported nonsense for a thread's core
(`svcGetProcessorID`) and for memory in use, where the console gave sane values.

## 12. Open questions

- Why a thread created on core 1 runs on core 2 (§4.4.1), and whether core 1 is worth having at
  all, given its share is capped.
- The PICA200's texture size and format limits as citro3d exposes them, and how much VRAM the
  chunk meshes may take.
- Whether 1.18+ world generation can run fast enough to keep ahead of walking (and flying) speed
  at a small render distance, and what to do when it can't.
- How far back worlds are upgraded (§6.2).
- Mojang's terms (checked 2026-10-23: the Minecraft EULA, the Usage Guidelines, the Microsoft
  Services Agreement) allow mods outright, and are relaxed about what a player makes for
  themselves. They don't address a third-party program that reimplements the game; nothing grants
  it, and the Usage Guidelines can be withdrawn without notice. A risk accepted, not solved. The
  name breaks the Usage Guidelines' naming rule ("Minecraft" mustn't be the dominant part of a
  title); kept by decision (§13).
- Trimming the mod's Rust jar to what it uses, and relocating `org.rustlang.*` (§3.1.2).
- Sign-in for online-mode servers (§9.2) still needs an Azure app Mojang has approved
  (`aka.ms/mce-reviewappid`); whether to apply, and when.
- Which releases after 1.21.11 to follow, and how quickly; in what order the older build bases
  (1.13–1.21.10) are added.

## 13. Decisions

- 2026-10-22 (user): a separate repository, `MinecraftNew3DSRevival`; exact Java parity in
  behaviour, keeping the bugs players rely on (breaking bedrock, TNT duping, quasi-connectivity);
  released as source with a build tool that needs a real copy of Minecraft and an account that
  owns it; Rust (or C++) over ctru; Java server compatibility, the big networks not aimed for;
  singleplayer first; sign-in by device code on the 3DS; No Chat Reports-style chat signing and
  control over what is sent; GPL-3.0.
- 2026-10-22: GPL-3.0-or-later (the FSF's recommended form; `-only` if preferred); the original 3DS
  not targeted (§1.5); worlds in Java's format (§6).
- 2026-10-23 (user): build bases are every Java release from 1.13 onward, and the game behaves as
  the chosen one (§1.1.1); 1.12.2 and earlier unsupported (the Flattening reorganised the data).
  1.21.11 first.
- 2026-10-23 (user): the build base is chosen at compile time (§5.6); `PARITY.md` tracks kept and
  fixed bugs with the releases each applies to; 1.13 to 1.14.3 are read through unofficial
  mappings (§2.3).
- 2026-10-23 (user): mappings only under CC0, MIT, BSD, GPL or MPL: Yarn and Feather, not MCP
  (§2.3); one Cargo feature per release (§5.6).
- 2026-10-23 (user): features spelled `mc1_21_11`; development starts with §14 step 1 and `core`'s
  foundations (Java's random generators) in parallel.
- 2026-10-23 (user): step 2 converts everything in §3.3. The game's files come from a mod in the
  player's own running game (Fabric, Legacy Fabric for 1.13.2, Ornithe for 1.13 and 1.13.1), which
  checks ownership with Mojang through the game's session; the player runs `n3dsr-build` on the
  export afterwards (§3). The mod is Rust compiled to JVM bytecode where that works, built per
  release by Stonecutter (§3.1.1). Mod Menu's config screen, a client command and a launch
  argument all start an export.
- 2026-10-23 (user): resource packs as Java takes them, from the game's own enabled list at build
  time and from the SD card at run time (§3.5); all textures in step 2c, RGB565 for opaque
  sprites and RGBA4 for the rest, every animation frame in the atlas, interpolation blended on
  the GPU, Java's mipmapping (§3.6).
- 2026-10-23 (user): the app's icon is the player's end stone texture, the small one averaged
  down from the large (§3.4.1).
- 2026-10-23 (user): the mod's proofs go in our config directory; its config screen waits for a
  "Check ownership" press; the launch argument is `-Dn3dsr.export=true`; Mod Menu is optional.
- 2026-10-23 (user): `rustc_codegen_jvm` is used for the mod (§3.1.1). The mod does only what
  proving ownership needs, checking it as online-mode servers do (`join`, `hasJoined`), and writes
  a proof; `n3dsr-build` reads the launcher-installed jar and assets itself (§3). The name stays,
  though it breaks the Usage Guidelines' naming rule (§12).
- 2026-10-23: the GPU through the `citro3d` Rust bindings, with `citro3d-sys` for what they lack
  (§10.1), after step 1 drew a textured cube in stereo on a New 3DS at 60 frames a second;
  threads on cores 2 and 3 (§4.4.1).

## 14. Steps

Each step its own commit, its tests passing.

1. **Toolchain**: a Rust app on ctru-rs drawing a textured cube through citro3d, on hardware and
   Azahar; the decision §10.1 leaves open.
2. **Ownership and build** (§3), in parts: (a) the plumbing: a Fabric mod for 1.21.11 whose Java
   entrypoint calls Rust compiled to JVM bytecode, reading something real from the game, remapped
   and running; (b) the mod for 1.21.11: Mod Menu, command, launch argument, the ownership check,
   the proof; (c) `n3dsr-build`: reading a proof, the jar and the assets; texture atlases;
   (d) models and blockstates; (e) sounds; (f) data packs, density functions as bytecode;
   (g) registries and language files; (h) the RomFS, `.3dsx` and `.cia`.
3. **A Java world on screen**: a world copied from a PC, its region files read (§6) and rendered,
   flown through; no simulation yet.
4. **Movement**: the player's physics (collision, gravity, sprinting, swimming), checked against
   recorded vanilla traces.
5. **Blocks and items**: breaking, placing, the inventory, crafting from the recipes.
6. **Ticking**: block updates, redstone, fluids, random ticks, in Java's order; the parity cases.
7. **World generation**: superflat, then noise; seed parity tests (§11.2). For 1.21.11 first.
8. **Entities and mobs**: AI, spawning, combat, the loot tables.
9. **Saving** (§6), so a world goes back to the PC.
10. **Java servers** (§9).
11. **Older build bases** (§1.1.1): back towards 1.13, each with its own parity tests.
