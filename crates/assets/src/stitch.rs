//! Packing an atlas's sprites into GPU pages (DESIGN.md §3.6). The layout is ours, not Java's:
//! only our renderer reads it. Each frame an animation uses is a tile; opaque tiles go to RGB565
//! pages and the rest to RGBA4 ones, up to the PICA200's 1024×1024, each page cut to the smallest
//! power of two around what it holds; tiles sit on a grid of `2^levels`, so each sprite's mip
//! levels are exactly its own pixels.

use serde::Serialize;

use crate::image::Image;
use crate::mipmap;
use crate::pica::Format;
use crate::sprite::Sprite;

/// The PICA200's largest texture side.
pub const MAX_SIDE: u32 = 1024;
/// Its smallest: one 8×8 tile.
const MIN_SIDE: u32 = 8;
/// Opaque sprites get pages of their own only with over 1/16 of the others' area…
const SPLIT_RATIO: u64 = 16;
/// …and at least a 64×64 page's worth.
const MIN_OPAQUE_AREA: u64 = 64 * 64;

/// A page: one GPU texture.
pub struct Page {
    pub format: Format,
    /// Level 0 first.
    pub levels: Vec<Image>,
}

impl Page {
    /// The page as the 3DS app loads it: `N3TX`, the format (0 RGB565, 1 RGBA4), the level
    /// count, width and height (u16, little-endian), then every level in the GPU's layout.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = b"N3TX".to_vec();
        out.push(match self.format {
            Format::Rgb565 => 0,
            Format::Rgba4 => 1,
        });
        out.push(self.levels.len() as u8);
        out.extend_from_slice(&(self.width() as u16).to_le_bytes());
        out.extend_from_slice(&(self.height() as u16).to_le_bytes());
        for level in &self.levels {
            out.extend(crate::pica::tile(level, self.format));
        }
        out
    }

    pub fn width(&self) -> u32 {
        self.levels[0].width
    }

    pub fn height(&self) -> u32 {
        self.levels[0].height
    }
}

/// Where a sprite's frames are, for the renderer.
#[derive(Serialize)]
pub struct Placement {
    pub id: String,
    pub page: usize,
    pub width: u32,
    pub height: u32,
    /// Each frame the animation uses (or the one frame of a still sprite): its top left on the
    /// page, by frame index.
    pub frames: Vec<(u32, [u32; 2])>,
    /// The animation, `None` when still.
    pub animation: Option<AnimationIndex>,
}

#[derive(Serialize)]
pub struct AnimationIndex {
    /// (frame index, ticks), in order.
    pub steps: Vec<(u32, u32)>,
    pub interpolate: bool,
}

/// An atlas, packed.
pub struct Stitched {
    pub pages: Vec<Page>,
    pub sprites: Vec<Placement>,
    pub levels: u32,
}

/// A page being filled: its format, its shelves, and which tile went where.
type OpenPage = (Format, Shelves, Vec<(usize, u32, u32)>);

/// One frame of one sprite at every level.
struct Tile {
    sprite: usize,
    frame: u32,
    levels: Vec<Image>,
    format: Format,
}

/// Packs `sprites` with up to `levels` mip levels, as many as Java would allow for them.
pub fn stitch(sprites: &[Sprite], levels: u32) -> Stitched {
    let levels = allowed_levels(sprites, levels);
    let align = 1 << levels;

    // Opaque sprites get RGB565 pages, unless they're so few (the missing texture alone, say) that
    // a page of their own would waste a texture: then they share the RGBA4 one.
    let opaque: Vec<bool> = sprites
        .iter()
        .map(|s| s.sheet.rgba.chunks(4).all(|p| p[3] == 255))
        .collect();
    let area = |want: bool| -> u64 {
        sprites
            .iter()
            .zip(&opaque)
            .filter(|(_, o)| **o == want)
            .map(|(s, _)| u64::from(s.sheet.width) * u64::from(s.sheet.height))
            .sum()
    };
    let (opaque_area, other_area) = (area(true), area(false));
    let split = other_area == 0
        || (opaque_area >= MIN_OPAQUE_AREA && opaque_area * SPLIT_RATIO > other_area);

    let mut tiles = Vec::new();
    for (index, sprite) in sprites.iter().enumerate() {
        let sheet_levels = mipmap::levels(sprite, levels);
        let format = if opaque[index] && split {
            Format::Rgb565
        } else {
            Format::Rgba4
        };
        let mut frames: Vec<u32> = match &sprite.animation {
            Some(animation) => animation.frames.iter().map(|f| f.index).collect(),
            None => vec![0],
        };
        frames.sort_unstable();
        frames.dedup();
        for frame in frames {
            let (x, y) = sprite.frame_origin(frame);
            let levels = sheet_levels
                .iter()
                .enumerate()
                .map(|(level, sheet)| {
                    let (w, h) = (
                        (sprite.frame_width >> level).max(1),
                        (sprite.frame_height >> level).max(1),
                    );
                    sheet.crop(x >> level, y >> level, w, h)
                })
                .collect();
            tiles.push(Tile {
                sprite: index,
                frame,
                levels,
                format,
            });
        }
    }

    // Tallest first, then widest, then by name, for a tight and repeatable packing.
    tiles.sort_by(|a, b| {
        let (sa, sb) = (&sprites[a.sprite], &sprites[b.sprite]);
        (sb.frame_height, sb.frame_width, &sa.id, a.frame).cmp(&(
            sa.frame_height,
            sa.frame_width,
            &sb.id,
            b.frame,
        ))
    });

    let mut pages: Vec<OpenPage> = Vec::new();
    for (tile_index, tile) in tiles.iter().enumerate() {
        let sprite = &sprites[tile.sprite];
        let (w, h) = (
            round_up(sprite.frame_width, align),
            round_up(sprite.frame_height, align),
        );
        let spot = pages
            .iter_mut()
            .enumerate()
            .filter(|(_, (format, ..))| *format == tile.format)
            .find_map(|(page, (_, shelves, _))| shelves.place(w, h).map(|at| (page, at)));
        let (page, (x, y)) = match spot {
            Some(spot) => spot,
            None => {
                let mut shelves = Shelves::default();
                let at = shelves.place(w, h).expect("a sprite larger than a page");
                pages.push((tile.format, shelves, Vec::new()));
                (pages.len() - 1, at)
            }
        };
        pages[page].2.push((tile_index, x, y));
    }

    let mut placements: Vec<Placement> = sprites
        .iter()
        .map(|sprite| Placement {
            id: sprite.id.clone(),
            page: 0,
            width: sprite.frame_width,
            height: sprite.frame_height,
            frames: Vec::new(),
            animation: sprite.animation.as_ref().map(|a| AnimationIndex {
                steps: a.frames.iter().map(|f| (f.index, f.ticks)).collect(),
                interpolate: a.interpolate,
            }),
        })
        .collect();

    let mut out_pages = Vec::new();
    for (page_index, (format, shelves, placed)) in pages.into_iter().enumerate() {
        let (width, height) = shelves.extent();
        let (width, height) = (page_side(width), page_side(height));
        // A level must stay a whole 8×8 tile for the GPU.
        let page_levels = levels.min((width.min(height) / MIN_SIDE).ilog2());
        let mut images: Vec<Image> = (0..=page_levels)
            .map(|l| Image::new(width >> l, height >> l))
            .collect();
        for (tile_index, x, y) in placed {
            let tile = &tiles[tile_index];
            for (level, image) in images.iter_mut().enumerate() {
                image.paste(&tile.levels[level], x >> level, y >> level);
            }
            let placement = &mut placements[tile.sprite];
            placement.page = page_index;
            placement.frames.push((tile.frame, [x, y]));
        }
        out_pages.push(Page {
            format,
            levels: images,
        });
    }
    for placement in &mut placements {
        placement.frames.sort_unstable();
    }
    Stitched {
        pages: out_pages,
        sprites: placements,
        levels,
    }
}

/// The sprites that hold an atlas below `wanted` mip levels, with the levels each allows, as
/// Java warns of them ("Texture … limits mip level from 4 to 3").
pub fn limiting_sprites(sprites: &[Sprite], wanted: u32) -> Vec<(&str, u32)> {
    sprites
        .iter()
        .filter_map(|sprite| {
            let allowed = allowed_levels(std::slice::from_ref(sprite), wanted);
            (allowed < wanted).then_some((sprite.id.as_str(), allowed))
        })
        .collect()
}

/// Java's limit (`SpriteLoader.stitch`): no more levels than the smallest sprite side, nor than
/// the smallest power of two dividing any sprite's sides.
pub fn allowed_levels(sprites: &[Sprite], wanted: u32) -> u32 {
    let mut smallest = u32::MAX;
    let mut divisor = 1u32 << wanted;
    for sprite in sprites {
        smallest = smallest.min(sprite.frame_width.min(sprite.frame_height));
        divisor = divisor
            .min(lowest_one_bit(sprite.frame_width).min(lowest_one_bit(sprite.frame_height)));
    }
    let limit = smallest.min(divisor);
    wanted.min(limit.max(1).ilog2())
}

fn lowest_one_bit(n: u32) -> u32 {
    n.isolate_lowest_one()
}

fn round_up(n: u32, to: u32) -> u32 {
    n.div_ceil(to) * to
}

/// The smallest power of two at least `n`, from one tile up.
fn page_side(n: u32) -> u32 {
    n.max(MIN_SIDE).next_power_of_two()
}

/// Shelf packing within one `MAX_SIDE` square: rows filled left to right, each as tall as its
/// first (tallest) tile.
#[derive(Default)]
struct Shelves {
    shelves: Vec<Shelf>,
    width_used: u32,
}

struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

impl Shelves {
    fn place(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        for shelf in &mut self.shelves {
            if h <= shelf.height && shelf.x + w <= MAX_SIDE {
                let at = (shelf.x, shelf.y);
                shelf.x += w;
                self.width_used = self.width_used.max(shelf.x);
                return Some(at);
            }
        }
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        if y + h > MAX_SIDE || w > MAX_SIDE {
            return None;
        }
        self.shelves.push(Shelf { y, height: h, x: w });
        self.width_used = self.width_used.max(w);
        Some((0, y))
    }

    fn extent(&self) -> (u32, u32) {
        (
            self.width_used,
            self.shelves.last().map_or(0, |s| s.y + s.height),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::{Animation, Frame};

    fn solid(id: &str, w: u32, h: u32, alpha: u8) -> Sprite {
        let mut image = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                image.set(x, y, [10, 20, 30, alpha]);
            }
        }
        Sprite::still(id.into(), image)
    }

    #[test]
    fn a_lone_opaque_sprite_shares_the_transparent_page() {
        let sprites = [
            solid("a:opaque", 16, 16, 255),
            solid("a:glass", 64, 64, 128),
        ];
        let stitched = stitch(&sprites, 0);
        assert_eq!(stitched.pages.len(), 1);
        assert_eq!(stitched.pages[0].format, Format::Rgba4);
    }

    #[test]
    fn opaque_and_transparent_go_to_their_own_pages() {
        let sprites = [
            solid("a:opaque", 64, 64, 255),
            solid("a:glass", 64, 64, 128),
        ];
        let stitched = stitch(&sprites, 0);
        assert_eq!(stitched.pages.len(), 2);
        let formats: Vec<Format> = stitched.pages.iter().map(|p| p.format).collect();
        assert!(formats.contains(&Format::Rgb565) && formats.contains(&Format::Rgba4));
        assert_ne!(stitched.sprites[0].page, stitched.sprites[1].page);
    }

    #[test]
    fn every_used_frame_is_a_tile() {
        let mut water = solid("a:water", 16, 64, 255);
        water.frame_height = 16;
        water.animation = Some(Animation {
            frames: vec![Frame { index: 0, ticks: 2 }, Frame { index: 2, ticks: 2 }],
            interpolate: false,
        });
        let stitched = stitch(&[water], 0);
        let frames: Vec<u32> = stitched.sprites[0].frames.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, [0, 2]);
    }

    #[test]
    fn a_small_sprite_limits_the_levels_as_in_java() {
        let sprites = [solid("a:big", 16, 16, 255), solid("a:small", 4, 4, 255)];
        assert_eq!(allowed_levels(&sprites, 4), 2);
        let odd = [solid("a:odd", 16, 12, 255)];
        assert_eq!(allowed_levels(&odd, 4), 2);
    }

    #[test]
    fn pages_are_powers_of_two_and_levels_whole_tiles() {
        let sprites: Vec<Sprite> = (0..40)
            .map(|i| solid(&format!("a:s{i:02}"), 16, 16, 255))
            .collect();
        let stitched = stitch(&sprites, 4);
        let page = &stitched.pages[0];
        assert!(page.width().is_power_of_two() && page.height().is_power_of_two());
        assert!(page.levels.iter().all(|l| l.width >= 8 && l.height >= 8));
    }
}
