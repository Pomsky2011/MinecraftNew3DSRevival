//! The app's icons, from the player's own end stone texture (DESIGN.md §3.4): the 48×48 large
//! icon by nearest-neighbour scaling, so the texture's pixels stay square, and the 24×24 small
//! icon by averaging the large one's 2×2 blocks.
//!
//! The small icon is made here rather than left to `smdhtool`, whose own averaging reads the top
//! left and bottom right pixels of each block twice and the other two never.

use anyhow::{Result, bail};
pub use n3dsr_assets::image::Image;

/// The texture, in the `minecraft` namespace, as in every release from 1.13 (which renamed
/// `blocks` to `block`).
pub const TEXTURE: &str = "textures/block/end_stone.png";
pub const LARGE: u32 = 48;
pub const SMALL: u32 = 24;

/// The large and small icons from the texture.
pub fn icons(texture: &Image) -> Result<(Image, Image)> {
    if texture.width != texture.height {
        bail!(
            "{TEXTURE} is {}×{}, not square",
            texture.width,
            texture.height
        );
    }
    let large = texture.scale_nearest(LARGE);
    let small = large.halve();
    debug_assert_eq!((small.width, small.height), (SMALL, SMALL));
    Ok((large, small))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_48_and_24() {
        let texture = Image::new(16, 16);
        let (large, small) = icons(&texture).unwrap();
        assert_eq!((large.width, small.width), (LARGE, SMALL));
    }
}
