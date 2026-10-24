//! The app's icons, from the player's own end stone texture (DESIGN.md §3.4): the 48×48 large
//! icon by nearest-neighbour scaling, so the texture's pixels stay square, and the 24×24 small
//! icon by averaging the large one's 2×2 blocks.
//!
//! The small icon is made here rather than left to `smdhtool`, whose own averaging reads the top
//! left and bottom right pixels of each block twice and the other two never.

use std::fs::File;
use std::io::{BufWriter, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};

/// The texture, in the `minecraft` namespace, as in every release from 1.13 (which renamed
/// `blocks` to `block`).
pub const TEXTURE: &str = "textures/block/end_stone.png";
pub const LARGE: u32 = 48;
pub const SMALL: u32 = 24;

/// An RGBA8 image, rows top first.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    fn texel(&self, x: u32, y: u32) -> &[u8] {
        let at = ((y * self.width + x) * 4) as usize;
        &self.rgba[at..at + 4]
    }

    pub fn decode_png(mut data: impl Read) -> Result<Self> {
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes)?;
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        // Palettes, low bit depths and missing alpha all become 8-bit RGBA.
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size().context("PNG too large")?];
        let info = reader.next_frame(&mut buffer)?;
        let pixels = &buffer[..info.buffer_size()];
        let rgba = match info.color_type {
            png::ColorType::Rgba => pixels.to_vec(),
            png::ColorType::Rgb => pixels
                .chunks(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => pixels
                .chunks(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => bail!("PNG palette wasn't expanded"),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            rgba,
        })
    }

    pub fn write_png(&self, path: &Path) -> Result<()> {
        let file = File::create(path).with_context(|| format!("writing {}", path.display()))?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&self.rgba)?;
        Ok(())
    }

    /// Nearest-neighbour to `size`×`size`.
    pub fn scale_nearest(&self, size: u32) -> Self {
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                rgba.extend_from_slice(self.texel(x * self.width / size, y * self.height / size));
            }
        }
        Self {
            width: size,
            height: size,
            rgba,
        }
    }

    /// Half the size, each pixel the average of a 2×2 block (rounded to nearest).
    pub fn halve(&self) -> Self {
        let (width, height) = (self.width / 2, self.height / 2);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let block = [
                    self.texel(2 * x, 2 * y),
                    self.texel(2 * x + 1, 2 * y),
                    self.texel(2 * x, 2 * y + 1),
                    self.texel(2 * x + 1, 2 * y + 1),
                ];
                for channel in 0..4 {
                    let sum: u32 = block.iter().map(|texel| u32::from(texel[channel])).sum();
                    rgba.push(((sum + 2) / 4) as u8);
                }
            }
        }
        Self {
            width,
            height,
            rgba,
        }
    }
}

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

    /// A 2×2 image: red, green / blue, white.
    fn quad() -> Image {
        Image {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        }
    }

    #[test]
    fn nearest_keeps_texels_square() {
        let big = quad().scale_nearest(6);
        assert_eq!((big.width, big.height), (6, 6));
        // Each texel covers a 3×3 block: (2, 2) is red, (3, 2) green, (2, 3) blue, (5, 5) white.
        assert_eq!(big.texel(2, 2), [255, 0, 0, 255]);
        assert_eq!(big.texel(3, 2), [0, 255, 0, 255]);
        assert_eq!(big.texel(2, 3), [0, 0, 255, 255]);
        assert_eq!(big.texel(5, 5), [255, 255, 255, 255]);
    }

    #[test]
    fn halving_averages_all_four() {
        let one = quad().halve();
        assert_eq!((one.width, one.height), (1, 1));
        // (255 + 0 + 0 + 255) / 4 rounds to 128 for red, likewise green and blue; alpha stays 255.
        assert_eq!(one.rgba, [128, 128, 128, 255]);
    }

    #[test]
    fn icons_are_48_and_24() {
        let texture = quad().scale_nearest(16);
        let (large, small) = icons(&texture).unwrap();
        assert_eq!((large.width, small.width), (LARGE, SMALL));
    }

    #[test]
    fn png_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("q.png");
        quad().write_png(&path).unwrap();
        assert_eq!(
            Image::decode_png(File::open(&path).unwrap()).unwrap(),
            quad()
        );
    }
}
