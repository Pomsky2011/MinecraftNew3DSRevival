# Parity list

Where this game's behaviour knowingly differs from Java Edition's, or matches a Java bug on purpose
(`DESIGN.md` §1.2). A Java bug not listed here is fixed. Each entry: the behaviour, Mojang's bug
number where there is one, and whether it is **kept** or **fixed**.

## Kept

| Behaviour | Java bug | Why |
| --- | --- | --- |
| Breaking bedrock (headless pistons, TNT and piston methods) | | Players build on it; Mojang treats it as a feature |
| TNT duplication | | Technical farms depend on it |
| Quasi-connectivity (pistons, droppers and dispensers powered as if one block higher) | MC-108 | Redstone designs depend on it; Mojang kept it |

## Fixed

None yet.
