//! Sprites: a texture as an atlas takes it, with its animation and its mipmap settings, read from
//! its `.mcmeta` as 1.21.11's `SpriteResourceLoader` and `SpriteContents` do (reimplemented).

use serde_json::Value;

use crate::image::Image;

/// `namespace:path`, the path without `textures/` or `.png`: `minecraft:block/stone`.
pub type Id = String;

/// How a sprite's mip levels are made (DESIGN.md §3.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MipmapStrategy {
    /// `Cutout` if any pixel is fully transparent, else `Mean`.
    Auto,
    Mean,
    Cutout,
    StrictCutout,
    DarkCutout,
}

/// One step of an animation: which frame of the sheet, for how many ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub index: u32,
    pub ticks: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub frames: Vec<Frame>,
    /// Blend each frame into the next, tick by tick.
    pub interpolate: bool,
}

/// A sprite: its sheet (every frame, left to right then down) and how to use it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub id: Id,
    pub frame_width: u32,
    pub frame_height: u32,
    pub sheet: Image,
    /// `None` for a still sprite, which shows the sheet's top-left frame.
    pub animation: Option<Animation>,
    pub mipmap_strategy: MipmapStrategy,
    pub alpha_cutoff_bias: f32,
    /// The `.mcmeta`'s other sections (a GUI sprite's `gui` scaling, say), for whoever needs them.
    pub extra: serde_json::Map<String, Value>,
}

impl Sprite {
    /// A sprite with no `.mcmeta`: one frame, the whole image.
    pub fn still(id: Id, image: Image) -> Self {
        Self {
            id,
            frame_width: image.width,
            frame_height: image.height,
            sheet: image,
            animation: None,
            mipmap_strategy: MipmapStrategy::Auto,
            alpha_cutoff_bias: 0.0,
            extra: serde_json::Map::new(),
        }
    }

    /// From a PNG and its `.mcmeta`. `Err` where Java leaves the sprite out (bad metadata, a sheet
    /// that isn't whole frames).
    pub fn load(id: Id, png: &[u8], mcmeta: Option<&[u8]>) -> Result<Self, String> {
        let meta: serde_json::Map<String, Value> = match mcmeta {
            Some(bytes) => {
                serde_json::from_slice(bytes).map_err(|e| format!("{id}: bad .mcmeta: {e}"))?
            }
            None => serde_json::Map::new(),
        };
        let image = Image::decode_png(png).map_err(|e| format!("{id}: {e}"))?;
        let animation = meta
            .get("animation")
            .map(AnimationMeta::read)
            .transpose()
            .map_err(|e| format!("{id}: {e}"))?;
        let texture = meta
            .get("texture")
            .map(TextureMeta::read)
            .transpose()
            .map_err(|e| format!("{id}: {e}"))?;

        let (frame_width, frame_height) = match &animation {
            Some(a) => a.frame_size(image.width, image.height),
            None => (image.width, image.height),
        };
        if image.width % frame_width != 0 || image.height % frame_height != 0 {
            return Err(format!(
                "{id}: size {}×{} is not a multiple of frame size {frame_width}×{frame_height}",
                image.width, image.height
            ));
        }
        let animation = animation
            .and_then(|a| a.animation(image.width / frame_width * (image.height / frame_height)));
        let texture = texture.unwrap_or_default();
        let mut extra = meta;
        extra.remove("animation");
        extra.remove("texture");
        Ok(Self {
            id,
            frame_width,
            frame_height,
            sheet: image,
            animation,
            mipmap_strategy: texture.mipmap_strategy,
            alpha_cutoff_bias: texture.alpha_cutoff_bias,
            extra,
        })
    }

    /// Frames in the sheet.
    pub fn frame_count(&self) -> u32 {
        (self.sheet.width / self.frame_width) * (self.sheet.height / self.frame_height)
    }

    /// Where frame `index` sits in the sheet.
    pub fn frame_origin(&self, index: u32) -> (u32, u32) {
        let per_row = self.sheet.width / self.frame_width;
        (
            (index % per_row) * self.frame_width,
            (index / per_row) * self.frame_height,
        )
    }
}

/// The `animation` section, as `AnimationMetadataSection`'s codec reads it.
struct AnimationMeta {
    frames: Option<Vec<(u32, Option<u32>)>>,
    width: Option<u32>,
    height: Option<u32>,
    frame_time: u32,
    interpolate: bool,
}

impl AnimationMeta {
    fn read(section: &Value) -> Result<Self, String> {
        let frames = section
            .get("frames")
            .map(|frames| {
                frames
                    .as_array()
                    .ok_or("animation frames isn't a list")?
                    .iter()
                    .map(|frame| match frame {
                        Value::Object(_) => Ok((
                            non_negative(
                                frame
                                    .get("index")
                                    .ok_or("an animation frame has no index")?,
                                "frame index",
                            )?,
                            frame
                                .get("time")
                                .map(|t| positive(t, "frame time"))
                                .transpose()?,
                        )),
                        index => Ok((non_negative(index, "frame index")?, None)),
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?;
        Ok(Self {
            frames,
            width: section
                .get("width")
                .map(|v| positive(v, "animation width"))
                .transpose()?,
            height: section
                .get("height")
                .map(|v| positive(v, "animation height"))
                .transpose()?,
            frame_time: section
                .get("frametime")
                .map(|v| positive(v, "frametime"))
                .transpose()?
                .unwrap_or(1),
            interpolate: section
                .get("interpolate")
                .map(|v| boolean(v, "interpolate"))
                .transpose()?
                .unwrap_or(false),
        })
    }

    /// `calculateFrameSize`: what's given, else square frames as wide as the sheet's shorter side.
    fn frame_size(&self, width: u32, height: u32) -> (u32, u32) {
        match (self.width, self.height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, height),
            (None, Some(h)) => (width, h),
            (None, None) => (width.min(height), width.min(height)),
        }
    }

    /// `createAnimatedTexture`: every frame in order unless `frames` says otherwise; steps with a
    /// bad index or time dropped (Java warns); `None` if one step or fewer is left.
    fn animation(self, frame_count: u32) -> Option<Animation> {
        let frames: Vec<Frame> = match self.frames {
            None => (0..frame_count)
                .map(|index| Frame {
                    index,
                    ticks: self.frame_time,
                })
                .collect(),
            Some(listed) => listed
                .into_iter()
                .map(|(index, ticks)| Frame {
                    index,
                    ticks: ticks.unwrap_or(self.frame_time),
                })
                .filter(|frame| frame.ticks > 0 && frame.index < frame_count)
                .collect(),
        };
        (frames.len() > 1).then_some(Animation {
            frames,
            interpolate: self.interpolate,
        })
    }
}

/// The `texture` section, as `TextureMetadataSection`'s codec reads it.
struct TextureMeta {
    mipmap_strategy: MipmapStrategy,
    alpha_cutoff_bias: f32,
}

impl Default for TextureMeta {
    fn default() -> Self {
        Self {
            mipmap_strategy: MipmapStrategy::Auto,
            alpha_cutoff_bias: 0.0,
        }
    }
}

impl TextureMeta {
    fn read(section: &Value) -> Result<Self, String> {
        // `blur` and `clamp` are read for their errors; the renderer decides filtering itself.
        for key in ["blur", "clamp"] {
            section.get(key).map(|v| boolean(v, key)).transpose()?;
        }
        let mipmap_strategy = match section.get("mipmap_strategy") {
            None => MipmapStrategy::Auto,
            Some(v) => match v.as_str() {
                Some("auto") => MipmapStrategy::Auto,
                Some("mean") => MipmapStrategy::Mean,
                Some("cutout") => MipmapStrategy::Cutout,
                Some("strict_cutout") => MipmapStrategy::StrictCutout,
                Some("dark_cutout") => MipmapStrategy::DarkCutout,
                _ => return Err(format!("unknown mipmap_strategy {v}")),
            },
        };
        let alpha_cutoff_bias = match section.get("alpha_cutoff_bias") {
            None => 0.0,
            Some(v) => v.as_f64().ok_or("alpha_cutoff_bias isn't a number")? as f32,
        };
        Ok(Self {
            mipmap_strategy,
            alpha_cutoff_bias,
        })
    }
}

fn non_negative(value: &Value, what: &str) -> Result<u32, String> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("{what} {value} isn't a non-negative int"))
}

fn positive(value: &Value, what: &str) -> Result<u32, String> {
    non_negative(value, what).and_then(|n| {
        if n > 0 {
            Ok(n)
        } else {
            Err(format!("{what} must be positive"))
        }
    })
}

fn boolean(value: &Value, what: &str) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{what} isn't true or false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG `width`×`height`, every pixel `rgba`.
    fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut image = Image::new(width, height);
        for y in 0..height {
            for x in 0..width {
                image.set(x, y, rgba);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("i.png");
        image.write_png(&path).unwrap();
        std::fs::read(path).unwrap()
    }

    #[test]
    fn a_tall_sheet_animates_every_frame_in_order() {
        let sprite = Sprite::load(
            "minecraft:block/x".into(),
            &png(16, 64, [1, 2, 3, 255]),
            Some(br#"{"animation": {"frametime": 2}}"#),
        )
        .unwrap();
        assert_eq!(
            (
                sprite.frame_width,
                sprite.frame_height,
                sprite.frame_count()
            ),
            (16, 16, 4)
        );
        assert_eq!(sprite.frame_origin(3), (0, 48));
        let animation = sprite.animation.unwrap();
        assert_eq!(animation.frames.len(), 4);
        assert_eq!(animation.frames[3], Frame { index: 3, ticks: 2 });
    }

    #[test]
    fn bad_steps_are_dropped_and_one_step_is_still() {
        let meta =
            br#"{"animation": {"frames": [0, {"index": 9, "time": 5}, {"index": 1, "time": 3}]}}"#;
        let sprite =
            Sprite::load("minecraft:block/x".into(), &png(16, 32, [0; 4]), Some(meta)).unwrap();
        assert_eq!(
            sprite.animation.unwrap().frames,
            [Frame { index: 0, ticks: 1 }, Frame { index: 1, ticks: 3 }]
        );
        let one = Sprite::load(
            "minecraft:block/x".into(),
            &png(16, 32, [0; 4]),
            Some(br#"{"animation": {"frames": [1]}}"#),
        )
        .unwrap();
        assert!(one.animation.is_none());
    }

    #[test]
    fn what_java_refuses_is_refused() {
        let ragged = Sprite::load(
            "minecraft:block/x".into(),
            &png(16, 40, [0; 4]),
            Some(br#"{"animation": {}}"#),
        );
        assert!(ragged.unwrap_err().contains("not a multiple"));
        let zero_time = Sprite::load(
            "minecraft:block/x".into(),
            &png(16, 32, [0; 4]),
            Some(br#"{"animation": {"frametime": 0}}"#),
        );
        assert!(zero_time.is_err());
        let strategy = Sprite::load(
            "minecraft:block/x".into(),
            &png(16, 16, [0; 4]),
            Some(br#"{"texture": {"mipmap_strategy": "dark_cutout", "alpha_cutoff_bias": 0.1}}"#),
        )
        .unwrap();
        assert_eq!(strategy.mipmap_strategy, MipmapStrategy::DarkCutout);
    }
}
