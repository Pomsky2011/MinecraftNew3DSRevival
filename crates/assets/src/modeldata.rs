//! Baked models as the 3DS app loads them (DESIGN.md §3.7): `models.bin`, `postcard` (serde, so
//! the build writes and the app reads the same types). Each distinct model and orientation is
//! baked once; blockstates and items point at them by index.

use serde::{Deserialize, Serialize};

use crate::bake::{Baked, Quad};
use crate::model::{Condition, Direction};

/// Bumped when the shape changes.
pub const FORMAT: u32 = 1;

/// Positions and UVs are stored as `i16` in units of 1/4096 (of a block, of a sprite).
pub const FIXED_ONE: f32 = 4096.0;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelData {
    pub format: u32,
    pub models: Vec<BakedModel>,
    pub blockstates: Vec<BlockStateData>,
    pub items: Vec<ItemData>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct BakedModel {
    pub ambient_occlusion: bool,
    pub quads: Vec<PackedQuad>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PackedQuad {
    pub positions: [[i16; 3]; 4],
    pub uvs: [[i16; 2]; 4],
    pub sprite: SpriteRef,
    pub tint: i8,
    /// Facing (bits 0–2), cull face (bits 3–5, 7 for none), shade (bit 6), light emission (bits
    /// 8–11).
    pub flags: u16,
}

/// Which atlas sprite a quad shows: an index into that atlas's sprites (its index JSON's order).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpriteRef {
    Blocks(u16),
    Items(u16),
    /// Neither atlas has it: the missing texture.
    Missing,
}

/// A weighted choice of baked model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Choice {
    pub model: u32,
    pub weight: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct BlockStateData {
    /// `namespace:path`, the blockstate file's (the block's) id.
    pub id: String,
    pub variants: Vec<(String, Vec<Choice>)>,
    pub multipart: Vec<(Option<Condition>, Vec<Choice>)>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ItemData {
    pub id: String,
    /// The item definition, minified: the game evaluates it.
    pub definition: String,
    /// Each model the definition names, and its baked index.
    pub models: Vec<(String, u32)>,
}

impl PackedQuad {
    pub fn pack(quad: &Quad, sprite: SpriteRef) -> Self {
        let fixed = |v: f32| {
            (v * FIXED_ONE)
                .round()
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16
        };
        let cull = quad.cull.map_or(7, |c| c as u16);
        Self {
            positions: quad.positions.map(|p| p.map(fixed)),
            uvs: quad.uvs.map(|uv| uv.map(fixed)),
            sprite,
            tint: quad.tint.clamp(-1, i8::MAX as i32) as i8,
            flags: quad.facing as u16
                | cull << 3
                | u16::from(quad.shade) << 6
                | u16::from(quad.light_emission) << 8,
        }
    }

    pub fn facing(&self) -> Direction {
        Direction::ALL[(self.flags & 7) as usize]
    }

    pub fn cull(&self) -> Option<Direction> {
        Direction::ALL.get((self.flags >> 3 & 7) as usize).copied()
    }

    pub fn shade(&self) -> bool {
        self.flags & 1 << 6 != 0
    }

    pub fn light_emission(&self) -> u8 {
        (self.flags >> 8 & 15) as u8
    }
}

impl BakedModel {
    pub fn pack(baked: &Baked, sprite: impl Fn(Option<&str>) -> SpriteRef) -> Self {
        Self {
            ambient_occlusion: baked.ambient_occlusion,
            quads: baked
                .quads
                .iter()
                .map(|q| PackedQuad::pack(q, sprite(q.texture.as_deref())))
                .collect(),
        }
    }
}

impl ModelData {
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("model data serialises")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let data: Self = postcard::from_bytes(bytes).map_err(|e| e.to_string())?;
        if data.format != FORMAT {
            return Err(format!(
                "models.bin is format {}, this reads {FORMAT}",
                data.format
            ));
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_pack_and_round_trip() {
        let quad = Quad {
            positions: [
                [0.0, 1.0, 0.5],
                [0.0625, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            uvs: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            texture: Some("minecraft:block/stone".into()),
            tint: 0,
            facing: Direction::North,
            cull: Some(Direction::North),
            shade: true,
            light_emission: 15,
        };
        let packed = PackedQuad::pack(&quad, SpriteRef::Blocks(7));
        assert_eq!(packed.positions[0], [0, 4096, 2048]);
        assert_eq!(packed.positions[1][0], 256);
        assert_eq!(
            (
                packed.facing(),
                packed.cull(),
                packed.shade(),
                packed.light_emission()
            ),
            (Direction::North, Some(Direction::North), true, 15)
        );
        let unculled = PackedQuad::pack(&Quad { cull: None, ..quad }, SpriteRef::Missing);
        assert_eq!(unculled.cull(), None);

        let data = ModelData {
            format: FORMAT,
            models: vec![BakedModel {
                ambient_occlusion: true,
                quads: vec![packed],
            }],
            blockstates: vec![BlockStateData {
                id: "minecraft:stone".into(),
                variants: vec![(
                    String::new(),
                    vec![Choice {
                        model: 0,
                        weight: 1,
                    }],
                )],
                multipart: Vec::new(),
            }],
            items: Vec::new(),
        };
        assert_eq!(ModelData::from_bytes(&data.to_bytes()).unwrap(), data);
    }
}
