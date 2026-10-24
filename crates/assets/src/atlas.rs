//! An atlas's sprites, from every pack's `atlases/<name>.json` and their sources (DESIGN.md §3.6),
//! as 1.21.11's `SpriteSourceList` and its sources gather them (reimplemented). Packing them into
//! pages is `pack`'s; this decides which sprites there are.

use std::collections::BTreeMap;
use std::io;

use regex::Regex;
use serde_json::Value;

use crate::image::Image;
use crate::sprite::{Id, Sprite};
use crate::stack::Stack;

/// An atlas's sprites, Java's missing-texture sprite first, then by id; and what went wrong on
/// the way, as Java would log it.
pub struct Atlas {
    pub name: String,
    pub sprites: Vec<Sprite>,
    pub notes: Vec<String>,
}

pub const MISSING: &str = "minecraft:missingno";

/// The atlases the game builds, and whether each gets mipmaps: a fixed list in the game's code
/// (`AtlasManager.KNOWN_ATLASES`), not whatever `atlases/` holds.
#[cfg(feature = "mc1_21_11")]
pub const KNOWN: &[(&str, bool)] = &[
    ("armor_trims", false),
    ("banner_patterns", false),
    ("beds", false),
    ("blocks", true),
    ("items", false),
    ("chests", false),
    ("decorated_pot", false),
    ("gui", false),
    ("map_decorations", false),
    ("paintings", false),
    ("particles", false),
    ("shield_patterns", false),
    ("shulker_boxes", false),
    ("signs", false),
    ("celestials", false),
];

/// A sprite found by a source, loaded only if no later source replaces or removes it.
enum Pending {
    File {
        namespace: String,
        path: String,
    },
    Region {
        namespace: String,
        path: String,
        region: Region,
    },
    Paletted {
        namespace: String,
        path: String,
        palette: usize,
    },
}

#[derive(Clone, Copy)]
struct Region {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    divisor_x: f64,
    divisor_y: f64,
}

/// A palette mapping: the key palette's colours (RGB) to a permutation's (ARGB).
type Palette = Result<BTreeMap<u32, u32>, String>;

pub fn load(stack: &Stack, name: &str) -> io::Result<Atlas> {
    let mut notes = Vec::new();
    let mut pending = BTreeMap::new();
    let mut palettes: Vec<Palette> = Vec::new();

    for definition in stack.read_stack("minecraft", &format!("atlases/{name}.json"))? {
        let sources = serde_json::from_slice::<Value>(&definition.bytes)
            .ok()
            .and_then(|v| v.get("sources").and_then(Value::as_array).cloned());
        let Some(sources) = sources else {
            notes.push(format!(
                "atlases/{name}.json in pack {} isn't a list of sources; skipped",
                stack.packs[definition.pack].id
            ));
            continue;
        };
        for source in &sources {
            if let Err(e) = run(stack, source, &mut pending, &mut palettes) {
                notes.push(format!("atlases/{name}.json: {e}"));
            }
        }
    }

    let mut sprites = vec![missing()];
    for (id, how) in pending {
        match load_pending(stack, id.clone(), how, &palettes) {
            Ok(sprite) => sprites.push(sprite),
            Err(e) => notes.push(e),
        }
    }
    Ok(Atlas {
        name: name.to_owned(),
        sprites,
        notes,
    })
}

/// Runs one source: adds sprites (a later one replacing an earlier with the same id) or removes
/// them.
fn run(
    stack: &Stack,
    source: &Value,
    pending: &mut BTreeMap<Id, Pending>,
    palettes: &mut Vec<Palette>,
) -> Result<(), String> {
    let kind = source
        .get("type")
        .and_then(Value::as_str)
        .ok_or("a source has no type")?;
    let string = |key: &str| {
        source
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{kind} source has no {key}"))
    };
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        "directory" => {
            let (directory, prefix) = (string("source")?, string("prefix")?);
            let root = format!("textures/{directory}");
            for (namespace, path) in stack.list_all(&root).map_err(|e| e.to_string())? {
                let Some(stem) = path.strip_suffix(".png") else {
                    continue;
                };
                let id = format!("{namespace}:{prefix}{}", &stem[root.len() + 1..]);
                pending.insert(id, Pending::File { namespace, path });
            }
        }
        "single" => {
            let resource = string("resource")?;
            let (namespace, path) = texture_file(resource);
            let sprite = source
                .get("sprite")
                .and_then(Value::as_str)
                .map_or_else(|| qualified(resource), qualified);
            if exists(stack, &namespace, &path) {
                pending.insert(sprite, Pending::File { namespace, path });
            } else {
                return Err(format!("missing sprite {namespace}:{path}"));
            }
        }
        "filter" => {
            let pattern = source
                .get("pattern")
                .ok_or("filter source has no pattern")?;
            let regex = |key: &str| -> Result<Option<Regex>, String> {
                pattern
                    .get(key)
                    .and_then(Value::as_str)
                    .map(|p| Regex::new(p).map_err(|e| format!("filter {key} {p:?}: {e}")))
                    .transpose()
            };
            let (namespace, path) = (regex("namespace")?, regex("path")?);
            pending.retain(|id, _| {
                let (ns, p) = id.split_once(':').unwrap();
                !(namespace.as_ref().is_none_or(|r| r.is_match(ns))
                    && path.as_ref().is_none_or(|r| r.is_match(p)))
            });
        }
        "unstitch" => {
            let (namespace, path) = texture_file(string("resource")?);
            if !exists(stack, &namespace, &path) {
                return Err(format!("missing sprite {namespace}:{path}"));
            }
            let number = |v: &Value, key: &str| {
                v.get(key)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("unstitch {key} isn't a number"))
            };
            let divisor_x = source
                .get("divisor_x")
                .map(|_| number(source, "divisor_x"))
                .transpose()?
                .unwrap_or(1.0);
            let divisor_y = source
                .get("divisor_y")
                .map(|_| number(source, "divisor_y"))
                .transpose()?
                .unwrap_or(1.0);
            let regions = source
                .get("regions")
                .and_then(Value::as_array)
                .filter(|r| !r.is_empty())
                .ok_or("unstitch has no regions")?;
            for r in regions {
                let sprite = r
                    .get("sprite")
                    .and_then(Value::as_str)
                    .ok_or("an unstitch region has no sprite")?;
                let region = Region {
                    x: number(r, "x")?,
                    y: number(r, "y")?,
                    width: number(r, "width")?,
                    height: number(r, "height")?,
                    divisor_x,
                    divisor_y,
                };
                pending.insert(
                    qualified(sprite),
                    Pending::Region {
                        namespace: namespace.clone(),
                        path: path.clone(),
                        region,
                    },
                );
            }
        }
        "paletted_permutations" => {
            let textures = source
                .get("textures")
                .and_then(Value::as_array)
                .ok_or("paletted_permutations has no textures")?;
            let key = palette(stack, string("palette_key")?);
            let separator = source
                .get("separator")
                .and_then(Value::as_str)
                .unwrap_or("_");
            let permutations = source
                .get("permutations")
                .and_then(Value::as_object)
                .ok_or("paletted_permutations has no permutations")?;
            let mut mapped = Vec::new();
            for (suffix, colours) in permutations {
                let colours = palette(stack, colours.as_str().ok_or("a permutation isn't an id")?);
                palettes.push(mapping(&key, &colours));
                mapped.push((suffix.clone(), palettes.len() - 1));
            }
            for texture in textures {
                let texture = texture.as_str().ok_or("a paletted texture isn't an id")?;
                let (namespace, path) = texture_file(texture);
                if !exists(stack, &namespace, &path) {
                    continue; // Java warns: "Unable to find texture".
                }
                for (suffix, palette) in &mapped {
                    pending.insert(
                        format!("{}{separator}{suffix}", qualified(texture)),
                        Pending::Paletted {
                            namespace: namespace.clone(),
                            path: path.clone(),
                            palette: *palette,
                        },
                    );
                }
            }
        }
        other => return Err(format!("unknown source type {other}")),
    }
    Ok(())
}

fn load_pending(
    stack: &Stack,
    id: Id,
    how: Pending,
    palettes: &[Palette],
) -> Result<Sprite, String> {
    let read = |namespace: &str, path: &str| -> Result<(Vec<u8>, usize), String> {
        let found = stack
            .read(namespace, path)
            .map_err(|e| format!("{id}: {e}"))?
            .ok_or_else(|| format!("{id}: {namespace}:{path} is gone"))?;
        Ok((found.bytes, found.pack))
    };
    match how {
        Pending::File { namespace, path } => {
            let (png, pack) = read(&namespace, &path)?;
            let meta = stack
                .metadata(&namespace, &path, pack)
                .map_err(|e| format!("{id}: {e}"))?;
            Sprite::load(id, &png, meta.as_deref())
        }
        Pending::Region {
            namespace,
            path,
            region,
        } => {
            let image = Image::decode_png(read(&namespace, &path)?.0.as_slice())
                .map_err(|e| format!("{id}: {e}"))?;
            // As Unstitcher: the region's fractions of the image, floored.
            let (w, h) = (
                image.width as f64 / region.divisor_x,
                image.height as f64 / region.divisor_y,
            );
            let (x, y) = ((region.x * w).floor() as u32, (region.y * h).floor() as u32);
            let (width, height) = (
                (region.width * w).floor() as u32,
                (region.height * h).floor() as u32,
            );
            if x + width > image.width || y + height > image.height || width == 0 || height == 0 {
                return Err(format!(
                    "{id}: failed to unstitch: region outside {namespace}:{path}"
                ));
            }
            Ok(Sprite::still(id, image.crop(x, y, width, height)))
        }
        Pending::Paletted {
            namespace,
            path,
            palette,
        } => {
            let mapping = palettes[palette]
                .as_ref()
                .map_err(|e| format!("{id}: unable to apply palette: {e}"))?;
            let mut image = Image::decode_png(read(&namespace, &path)?.0.as_slice())
                .map_err(|e| format!("{id}: {e}"))?;
            for y in 0..image.height {
                for x in 0..image.width {
                    image.set(x, y, recolour(image.texel(x, y), mapping));
                }
            }
            Ok(Sprite::still(id, image))
        }
    }
}

/// `PalettedPermutations`' mapping: pixels whose colour (alpha aside) is in the key palette take
/// the permutation's colour, keeping their own alpha scaled by the new colour's; others become
/// opaque; transparent ones are left alone.
fn recolour([r, g, b, a]: [u8; 4], mapping: &BTreeMap<u32, u32>) -> [u8; 4] {
    if a == 0 {
        return [r, g, b, a];
    }
    let rgb = u32::from_be_bytes([0, r, g, b]);
    let argb = mapping.get(&rgb).copied().unwrap_or(0xFF00_0000 | rgb);
    let [new_a, new_r, new_g, new_b] = argb.to_be_bytes();
    [
        new_r,
        new_g,
        new_b,
        (u32::from(a) * u32::from(new_a) / 255) as u8,
    ]
}

/// A palette image's pixels, row by row, as ARGB.
fn palette(stack: &Stack, id: &str) -> Result<Vec<u32>, String> {
    let (namespace, path) = texture_file(id);
    let found = stack
        .read(&namespace, &path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("failed to load palette image {id}"))?;
    let image = Image::decode_png(found.bytes.as_slice()).map_err(|e| e.to_string())?;
    Ok(image
        .rgba
        .chunks(4)
        .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
        .collect())
}

fn mapping(key: &Result<Vec<u32>, String>, colours: &Result<Vec<u32>, String>) -> Palette {
    let (key, colours) = (
        key.as_ref().map_err(Clone::clone)?,
        colours.as_ref().map_err(Clone::clone)?,
    );
    if key.len() != colours.len() {
        return Err(format!(
            "palette mapping has different sizes: {} and {}",
            key.len(),
            colours.len()
        ));
    }
    Ok(key
        .iter()
        .zip(colours)
        .filter(|(k, _)| *k >> 24 != 0)
        .map(|(k, c)| (k & 0x00FF_FFFF, *c))
        .collect())
}

/// Java's missing texture: 16×16, magenta and black quarters.
pub fn missing() -> Sprite {
    let mut image = Image::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let magenta = (y < 8) ^ (x < 8);
            image.set(
                x,
                y,
                if magenta {
                    [0xF8, 0x00, 0xF8, 0xFF]
                } else {
                    [0, 0, 0, 0xFF]
                },
            );
        }
    }
    Sprite::still(MISSING.into(), image)
}

/// `ns:path` or `path` (in `minecraft`), qualified.
fn qualified(id: &str) -> Id {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

/// A texture id's file: `minecraft:block/stone` → (`minecraft`, `textures/block/stone.png`).
fn texture_file(id: &str) -> (String, String) {
    let id = qualified(id);
    let (namespace, path) = id.split_once(':').unwrap();
    (namespace.to_owned(), format!("textures/{path}.png"))
}

fn exists(stack: &Stack, namespace: &str, path: &str) -> bool {
    matches!(stack.read(namespace, path), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::pack::Pack;
    use crate::source::Folder;

    fn png(image: &Image) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("i.png");
        image.write_png(&path).unwrap();
        fs::read(path).unwrap()
    }

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Image {
        let mut image = Image::new(width, height);
        for y in 0..height {
            for x in 0..width {
                image.set(x, y, rgba);
            }
        }
        image
    }

    /// A vanilla-like pack folder holding `files`.
    fn stack_of(root: &std::path::Path, files: &[(&str, Vec<u8>)]) -> Stack {
        for (path, bytes) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        Stack {
            packs: vec![Pack::built_in(
                "vanilla".into(),
                Box::new(Folder::new(root.to_owned())),
            )],
        }
    }

    #[test]
    fn sources_add_replace_and_remove_sprites() {
        let dir = tempfile::tempdir().unwrap();
        let stack = stack_of(dir.path(), &[
            ("assets/minecraft/textures/block/stone.png", png(&solid(16, 16, [1, 1, 1, 255]))),
            ("assets/minecraft/textures/block/dirt.png", png(&solid(16, 16, [2, 2, 2, 255]))),
            ("assets/minecraft/textures/entity/bell.png", png(&solid(32, 16, [3, 3, 3, 255]))),
            ("assets/minecraft/atlases/blocks.json", br#"{"sources": [
                {"type": "minecraft:directory", "source": "block", "prefix": "block/"},
                {"type": "single", "resource": "entity/bell", "sprite": "minecraft:block/bell"},
                {"type": "filter", "pattern": {"path": "dirt"}},
                {"type": "unstitch", "resource": "minecraft:entity/bell", "divisor_x": 2, "regions": [
                    {"sprite": "half", "x": 1, "y": 0, "width": 1, "height": 1}]}
            ]}"#.to_vec()),
        ]);
        let atlas = load(&stack, "blocks").unwrap();
        assert!(atlas.notes.is_empty(), "{:?}", atlas.notes);
        let ids: Vec<&str> = atlas.sprites.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                MISSING,
                "minecraft:block/bell",
                "minecraft:block/stone",
                "minecraft:half"
            ]
        );
        // The right half of the 32×16 bell.
        assert_eq!(
            (atlas.sprites[3].sheet.width, atlas.sprites[3].sheet.height),
            (16, 16)
        );
    }

    #[test]
    fn palettes_recolour_by_rgb_keeping_alpha() {
        let mut key = Image::new(2, 1);
        key.set(0, 0, [10, 10, 10, 255]);
        key.set(1, 0, [20, 20, 20, 255]);
        let mut gold = Image::new(2, 1);
        gold.set(0, 0, [200, 150, 0, 255]);
        gold.set(1, 0, [100, 75, 0, 128]);
        let mut trim = Image::new(3, 1);
        trim.set(0, 0, [10, 10, 10, 255]);
        trim.set(1, 0, [20, 20, 20, 200]);
        trim.set(2, 0, [99, 99, 99, 255]);
        let dir = tempfile::tempdir().unwrap();
        let stack = stack_of(dir.path(), &[
            ("assets/minecraft/textures/trims/palette.png", png(&key)),
            ("assets/minecraft/textures/trims/gold.png", png(&gold)),
            ("assets/minecraft/textures/trims/coast.png", png(&trim)),
            ("assets/minecraft/atlases/armor_trims.json", br#"{"sources": [{"type": "paletted_permutations",
                "textures": ["trims/coast"], "palette_key": "trims/palette", "permutations": {"gold": "trims/gold"}}]}"#.to_vec()),
        ]);
        let atlas = load(&stack, "armor_trims").unwrap();
        let sprite = &atlas.sprites[1];
        assert_eq!(sprite.id, "minecraft:trims/coast_gold");
        assert_eq!(sprite.sheet.texel(0, 0), [200, 150, 0, 255]);
        // 200 × 128 / 255 = 100.
        assert_eq!(sprite.sheet.texel(1, 0), [100, 75, 0, 100]);
        assert_eq!(sprite.sheet.texel(2, 0), [99, 99, 99, 255]);
    }
}
