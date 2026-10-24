//! Textures drawn on their own rather than from an atlas (DESIGN.md §3.6): mobs, the sky, fonts,
//! overlays. Java draws these as they are (no mipmaps), with the `.mcmeta` `texture` section's
//! `blur` and `clamp`; here each becomes one GPU texture, padded out to the GPU's power-of-two
//! sides with its pixels top left, so every pixel stays exact. Three exceptions (decided §13):
//! Realms' textures are left out, the title panorama is halved, and a texture taller than the
//! GPU allows is cut into strips.

use serde::Serialize;
use serde_json::Value;

use crate::image::Image;
use crate::mipmap;
use crate::pica::Format;
use crate::stack::Stack;
use crate::stitch::{MAX_SIDE, Page};

/// The colour maps: drawn by nobody, but read by the game for biome tints; kept as data too.
pub const COLORMAP_DIR: &str = "textures/colormap/";

/// The title screen's panorama faces, halved (1024 to 512): the top screen shows some 420 pixels
/// of a face across, so nothing visible is lost.
const PANORAMA: &str = "textures/gui/title/background/panorama_";

pub struct Standalone {
    pub id: String,
    /// One page, or several strips top to bottom when the texture is taller than the GPU allows.
    pub pages: Vec<Page>,
    pub entry: Entry,
}

/// What the renderer needs to know about one, beside its pixels.
#[derive(Serialize)]
pub struct Entry {
    /// The texture's size as drawn (after any halving); each page around it is a power of two.
    pub width: u32,
    pub height: u32,
    pub blur: bool,
    pub clamp: bool,
    /// Where each strip starts, in rows, when there's more than one page.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub strips: Vec<u32>,
}

/// Whether the build leaves a texture out: Realms isn't on the 3DS.
pub fn left_out(namespace: &str, path: &str) -> bool {
    namespace == "realms" || path.starts_with("textures/gui/realms/")
}

/// The texture at `namespace`/`path` (`textures/…png`), or why there's none. `row_height` is the
/// glyph row height when a bitmap font uses it, so strips never cut a glyph.
pub fn load(
    stack: &Stack,
    namespace: &str,
    path: &str,
    row_height: Option<u32>,
) -> Result<Standalone, String> {
    let id = format!(
        "{namespace}:{}",
        &path["textures/".len()..path.len() - ".png".len()]
    );
    let found = stack
        .read(namespace, path)
        .map_err(|e| format!("{id}: {e}"))?
        .ok_or_else(|| format!("{id}: gone"))?;
    let mut image = Image::decode_png(found.bytes.as_slice()).map_err(|e| format!("{id}: {e}"))?;
    if path.starts_with(PANORAMA) && image.width > 512 {
        image = mipmap::halve_linear(&image);
    }
    if image.width > MAX_SIDE {
        return Err(format!(
            "{id}: {} wide, more than the GPU's {MAX_SIDE}; left out",
            image.width
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
    let strip = match row_height {
        Some(row) if row > 0 && row <= MAX_SIDE => MAX_SIDE / row * row,
        _ => MAX_SIDE,
    };
    let starts: Vec<u32> = (0..image.height).step_by(strip as usize).collect();
    let side = |n: u32| n.max(8).next_power_of_two();
    let pages = starts
        .iter()
        .map(|&top| {
            let rows = strip.min(image.height - top);
            let mut padded = Image::new(side(image.width), side(rows));
            padded.paste(&image.crop(0, top, image.width, rows), 0, 0);
            Page {
                format,
                levels: vec![padded],
            }
        })
        .collect();
    Ok(Standalone {
        id,
        pages,
        entry: Entry {
            width: image.width,
            height: image.height,
            blur,
            clamp,
            strips: if starts.len() > 1 { starts } else { Vec::new() },
        },
    })
}

/// Each bitmap font texture's glyph row height (its height over its rows of characters), from
/// every pack's `font/*.json`.
pub fn font_row_heights(stack: &Stack) -> std::io::Result<Vec<((String, String), u32)>> {
    let mut heights = Vec::new();
    for (namespace, path) in stack.list_all("font")? {
        if !path.ends_with(".json") {
            continue;
        }
        for definition in stack.read_stack(&namespace, &path)? {
            let Ok(json) = serde_json::from_slice::<Value>(&definition.bytes) else {
                continue;
            };
            for provider in json
                .get("providers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let bitmap = provider
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.ends_with("bitmap"));
                let (Some(file), Some(rows)) = (
                    provider.get("file").and_then(Value::as_str),
                    provider
                        .get("chars")
                        .and_then(Value::as_array)
                        .map(Vec::len),
                ) else {
                    continue;
                };
                if !bitmap || rows == 0 {
                    continue;
                }
                let (ns, file) = file.split_once(':').unwrap_or(("minecraft", file));
                let texture = (ns.to_owned(), format!("textures/{file}"));
                let Ok(Some(found)) = stack.read(&texture.0, &texture.1) else {
                    continue;
                };
                let Ok(image) = Image::decode_png(found.bytes.as_slice()) else {
                    continue;
                };
                heights.push((texture, image.height / rows as u32));
            }
        }
    }
    Ok(heights)
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
    use std::path::Path;

    use super::*;
    use crate::pack::Pack;
    use crate::source::Folder;

    fn stack_with(root: &Path, files: &[(&str, &Image)]) -> Stack {
        for (path, image) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            image.write_png(&file).unwrap();
        }
        Stack {
            packs: vec![Pack::built_in(
                "vanilla".into(),
                Box::new(Folder::new(root.to_owned())),
            )],
        }
    }

    #[test]
    fn odd_sizes_are_padded_top_left() {
        let dir = tempfile::tempdir().unwrap();
        let mut fox = Image::new(48, 32);
        fox.set(47, 31, [1, 2, 3, 255]);
        let stack = stack_with(
            dir.path(),
            &[("assets/minecraft/textures/entity/fox/fox.png", &fox)],
        );
        fs::write(
            dir.path()
                .join("assets/minecraft/textures/entity/fox/fox.png.mcmeta"),
            br#"{"texture": {"blur": true}}"#,
        )
        .unwrap();
        let fox = load(&stack, "minecraft", "textures/entity/fox/fox.png", None).unwrap();
        assert_eq!(fox.id, "minecraft:entity/fox/fox");
        assert_eq!((fox.pages[0].width(), fox.pages[0].height()), (64, 32));
        assert_eq!(
            (
                fox.entry.width,
                fox.entry.height,
                fox.entry.blur,
                fox.entry.clamp
            ),
            (48, 32, true, false)
        );
        assert_eq!(fox.pages[0].levels[0].texel(47, 31), [1, 2, 3, 255]);
        assert_eq!(fox.pages[0].format, Format::Rgba4);
    }

    #[test]
    fn tall_font_sheets_split_on_glyph_rows() {
        let dir = tempfile::tempdir().unwrap();
        let sheet = Image::new(288, 1800);
        let stack = stack_with(
            dir.path(),
            &[("assets/minecraft/textures/font/accented.png", &sheet)],
        );
        let font = load(&stack, "minecraft", "textures/font/accented.png", Some(24)).unwrap();
        // 42 rows of 24 fit in 1024: strips at 0 and 1008.
        assert_eq!(font.entry.strips, [0, 1008]);
        let sizes: Vec<(u32, u32)> = font.pages.iter().map(|p| (p.width(), p.height())).collect();
        assert_eq!(sizes, [(512, 1024), (512, 1024)]);
    }

    #[test]
    fn the_panorama_is_halved_and_realms_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let face = Image::new(1024, 1024);
        let stack = stack_with(
            dir.path(),
            &[(
                "assets/minecraft/textures/gui/title/background/panorama_0.png",
                &face,
            )],
        );
        let panorama = load(
            &stack,
            "minecraft",
            "textures/gui/title/background/panorama_0.png",
            None,
        )
        .unwrap();
        assert_eq!(
            (panorama.entry.width, panorama.pages[0].width()),
            (512, 512)
        );
        assert!(left_out("realms", "textures/gui/images/1.png"));
        assert!(left_out("minecraft", "textures/gui/realms/no_realms.png"));
        assert!(!left_out("minecraft", "textures/gui/title/minecraft.png"));
    }
}
