# Parity list

Where this game's behaviour knowingly differs from Java Edition's, or matches a Java bug on purpose
(`DESIGN.md` §1.2). A Java bug not listed here is fixed. Each entry: the behaviour, Mojang's bug
number where there is one, the build bases it applies to (a bug may exist only in some releases,
`DESIGN.md` §1.1.1), and whether it is **kept** or **fixed**.

Releases are written as ranges: `1.13–` (1.13 onward), `1.13–1.17.1`, `1.20.5`. A range ends where
Mojang fixed or introduced the bug; the build base decides whether an entry applies.

## Kept

| Behaviour | Java bug | Releases | Why |
| --- | --- | --- | --- |
| Breaking bedrock (headless pistons, TNT and piston methods) | | 1.13– (to confirm) | Players build on it; Mojang treats it as a feature |
| TNT duplication | | 1.13– (to confirm) | Technical farms depend on it |
| Quasi-connectivity (pistons, droppers and dispensers powered as if one block higher) | MC-108 | 1.13– | Redstone designs depend on it; Mojang kept it |

## Fixed

| Behaviour | Java bug | Releases | How |
| --- | --- | --- | --- |
