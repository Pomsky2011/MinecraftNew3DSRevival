//! RGBA8 images: decoding PNGs, writing them (previews, icons), and the pixel work the textures
//! need.

use std::fs::File;
use std::io::{self, BufWriter, Read};
use std::path::Path;

/// An RGBA8 image, rows top first.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// All transparent black.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn texel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.width + x) * 4) as usize;
        self.rgba[at..at + 4].try_into().unwrap()
    }

    pub fn set(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let at = ((y * self.width + x) * 4) as usize;
        self.rgba[at..at + 4].copy_from_slice(&rgba);
    }

    /// The `width`×`height` rectangle at (`x`, `y`).
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        let mut out = Self::new(width, height);
        for row in 0..height {
            let from = (((y + row) * self.width + x) * 4) as usize;
            let to = (row * width * 4) as usize;
            let len = (width * 4) as usize;
            out.rgba[to..to + len].copy_from_slice(&self.rgba[from..from + len]);
        }
        out
    }

    /// Copies `other` in with its top left at (`x`, `y`).
    pub fn paste(&mut self, other: &Self, x: u32, y: u32) {
        for row in 0..other.height {
            let from = (row * other.width * 4) as usize;
            let to = (((y + row) * self.width + x) * 4) as usize;
            let len = (other.width * 4) as usize;
            self.rgba[to..to + len].copy_from_slice(&other.rgba[from..from + len]);
        }
    }

    pub fn decode_png(mut data: impl Read) -> io::Result<Self> {
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes)?;
        let mut decoder = png::Decoder::new(io::Cursor::new(bytes));
        // Palettes, low bit depths and missing alpha all become 8-bit RGBA.
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(io::Error::other)?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| io::Error::other("PNG too large"))?;
        let mut buffer = vec![0; size];
        let info = reader.next_frame(&mut buffer).map_err(io::Error::other)?;
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
            png::ColorType::Indexed => return Err(io::Error::other("PNG palette wasn't expanded")),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            rgba,
        })
    }

    pub fn write_png(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path)?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(io::Error::other)?;
        writer
            .write_image_data(&self.rgba)
            .map_err(io::Error::other)
    }

    /// Nearest-neighbour to `size`×`size`.
    pub fn scale_nearest(&self, size: u32) -> Self {
        let mut out = Self::new(size, size);
        for y in 0..size {
            for x in 0..size {
                out.set(
                    x,
                    y,
                    self.texel(x * self.width / size, y * self.height / size),
                );
            }
        }
        out
    }

    /// Half the size, each pixel the plain average of a 2×2 block (rounded to nearest). Not
    /// Java's mipmapping, which is `mipmap`'s.
    pub fn halve(&self) -> Self {
        let mut out = Self::new(self.width / 2, self.height / 2);
        for y in 0..out.height {
            for x in 0..out.width {
                let block = [
                    self.texel(2 * x, 2 * y),
                    self.texel(2 * x + 1, 2 * y),
                    self.texel(2 * x, 2 * y + 1),
                    self.texel(2 * x + 1, 2 * y + 1),
                ];
                let mut texel = [0; 4];
                for (channel, value) in texel.iter_mut().enumerate() {
                    let sum: u32 = block.iter().map(|t| u32::from(t[channel])).sum();
                    *value = ((sum + 2) / 4) as u8;
                }
                out.set(x, y, texel);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×2 image: red, green / blue, white.
    pub fn quad() -> Image {
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
        // Each texel covers a 3×3 block.
        assert_eq!(big.texel(2, 2), [255, 0, 0, 255]);
        assert_eq!(big.texel(3, 2), [0, 255, 0, 255]);
        assert_eq!(big.texel(2, 3), [0, 0, 255, 255]);
        assert_eq!(big.texel(5, 5), [255, 255, 255, 255]);
    }

    #[test]
    fn halving_averages_all_four() {
        let one = quad().halve();
        // (255 + 0 + 0 + 255) / 4 rounds to 128 for each colour; alpha stays 255.
        assert_eq!(one.rgba, [128, 128, 128, 255]);
    }

    #[test]
    fn crop_and_paste_round_trip() {
        let big = quad().scale_nearest(4);
        let corner = big.crop(2, 0, 2, 2);
        assert_eq!(corner.texel(0, 0), [0, 255, 0, 255]);
        let mut canvas = Image::new(4, 4);
        canvas.paste(&corner, 1, 1);
        assert_eq!(canvas.texel(1, 1), [0, 255, 0, 255]);
        assert_eq!(canvas.texel(0, 0), [0, 0, 0, 0]);
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
