//! Textures drawn on their own rather than from an atlas (DESIGN.md §3.6): mobs, the sky, fonts,
//! overlays. Java draws these as they are (no mipmaps), with the `.mcmeta` `texture` section's
//! `blur` and `clamp`; here each becomes one GPU texture, padded out to the GPU's power-of-two
//! sides with its pixels top left, so every pixel stays exact.

use serde::Serialize;
use serde_json::Value;

use crate::image::Image;
use crate::pica::Format;
use crate::stack::Stack;
use crate::stitch::{MAX_SIDE, Page};

/// The colour maps: drawn by nobody, but read by the game for biome tints; kept as data too.
pub const COLORMAP_DIR: &str = "textures/colormap/";

pub struct Standalone {
    pub id: String,
    pub page: Page,
    pub entry: Entry,
}

/// What the renderer needs to know about one, beside its pixels.
#[derive(Serialize)]
pub struct Entry {
    /// The texture's own size; the page around it is a power of two.
    pub width: u32,
    pub height: u32,
    pub blur: bool,
    pub clamp: bool,
}

/// The texture at `namespace`/`path` (`textures/…png`), or why there's none.
pub fn load(stack: &Stack, namespace: &str, path: &str) -> Result<Standalone, String> {
    let id = format!(
        "{namespace}:{}",
        &path["textures/".len()..path.len() - ".png".len()]
    );
    let found = stack
        .read(namespace, path)
        .map_err(|e| format!("{id}: {e}"))?
        .ok_or_else(|| format!("{id}: gone"))?;
    let image = Image::decode_png(found.bytes.as_slice()).map_err(|e| format!("{id}: {e}"))?;
    if image.width > MAX_SIDE || image.height > MAX_SIDE {
        return Err(format!(
            "{id}: {}×{} is larger than the GPU's {MAX_SIDE}×{MAX_SIDE}; left out",
            image.width, image.height
        ));
    }
    let meta = stack
        .metadata(namespace, path, found.pack)
        .map_err(|e| format!("{id}: {e}"))?;
    let (blur, clamp) = texture_flags(meta.as_deref());

    let format = if image.rgba.chunks(4).all(|p| p[3] == 255) {
        Format::Rgb565
    } else {
        Format::Rgba4
    };
    let side = |n: u32| n.max(8).next_power_of_two();
    let mut padded = Image::new(side(image.width), side(image.height));
    padded.paste(&image, 0, 0);
    Ok(Standalone {
        id,
        page: Page {
            format,
            levels: vec![padded],
        },
        entry: Entry {
            width: image.width,
            height: image.height,
            blur,
            clamp,
        },
    })
}

/// A colour map as the game reads it: rows of RGB, top first.
pub fn colormap_rgb(image: &Image) -> Vec<u8> {
    image
        .rgba
        .chunks(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect()
}

/// `blur` and `clamp` from a `.mcmeta`'s `texture` section; false when missing or malformed, as
/// Java falls back.
fn texture_flags(mcmeta: Option<&[u8]>) -> (bool, bool) {
    let section = mcmeta
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
        .and_then(|meta| meta.get("texture").cloned());
    let flag = |key: &str| {
        section
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    (flag("blur"), flag("clamp"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::pack::Pack;
    use crate::source::Folder;

    #[test]
    fn odd_sizes_are_padded_top_left() {
        let dir = tempfile::tempdir().unwrap();
        let mut fox = Image::new(48, 32);
        fox.set(47, 31, [1, 2, 3, 255]);
        let file = dir
            .path()
            .join("assets/minecraft/textures/entity/fox/fox.png");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fox.write_png(&file).unwrap();
        fs::write(
            file.with_extension("png.mcmeta"),
            br#"{"texture": {"blur": true}}"#,
        )
        .unwrap();
        let stack = Stack {
            packs: vec![Pack::built_in(
                "vanilla".into(),
                Box::new(Folder::new(dir.path().to_owned())),
            )],
        };
        let fox = load(&stack, "minecraft", "textures/entity/fox/fox.png").unwrap();
        assert_eq!(fox.id, "minecraft:entity/fox/fox");
        assert_eq!((fox.page.width(), fox.page.height()), (64, 32));
        assert_eq!(
            (
                fox.entry.width,
                fox.entry.height,
                fox.entry.blur,
                fox.entry.clamp
            ),
            (48, 32, true, false)
        );
        assert_eq!(fox.page.levels[0].texel(47, 31), [1, 2, 3, 255]);
        assert_eq!(fox.page.format, Format::Rgba4);
    }
}
