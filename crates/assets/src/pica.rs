//! The PICA200's texture formats and layout (DESIGN.md §3.6, §7.3): 16-bit RGB565 and RGBA4,
//! stored in 8×8 tiles, Morton order inside each, rows from the bottom up; mip levels one after
//! another. The layout is the one `n3dsr-client` drew its first cube with.

use crate::image::Image;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Opaque: 5 bits red, 6 green, 5 blue.
    Rgb565,
    /// With transparency: 4 bits each.
    Rgba4,
}

impl Format {
    /// A pixel in this format, rounded to nearest.
    pub fn encode(self, [r, g, b, a]: [u8; 4]) -> u16 {
        let scale = |value: u8, max: u16| (u16::from(value) * max + 127) / 255;
        match self {
            Self::Rgb565 => scale(r, 31) << 11 | scale(g, 63) << 5 | scale(b, 31),
            Self::Rgba4 => {
                scale(r, 15) << 12 | scale(g, 15) << 8 | scale(b, 15) << 4 | scale(a, 15)
            }
        }
    }

    /// What the GPU will show for a pixel in this format (bits replicated up to 8).
    pub fn decode(self, pixel: u16) -> [u8; 4] {
        let expand = |value: u16, bits: u32| {
            let value = value as u32;
            ((value << (8 - bits)) | (value >> (2 * bits - 8))) as u8
        };
        match self {
            Self::Rgb565 => [
                expand(pixel >> 11, 5),
                expand(pixel >> 5 & 0x3F, 6),
                expand(pixel & 0x1F, 5),
                255,
            ],
            Self::Rgba4 => [
                expand(pixel >> 12, 4),
                expand(pixel >> 8 & 0xF, 4),
                expand(pixel >> 4 & 0xF, 4),
                expand(pixel & 0xF, 4),
            ],
        }
    }
}

/// One level in the GPU's layout, little-endian 16-bit texels. Sides must be multiples of 8.
pub fn tile(image: &Image, format: Format) -> Vec<u8> {
    assert!(
        image.width.is_multiple_of(8) && image.height.is_multiple_of(8),
        "{}×{} isn't whole tiles",
        image.width,
        image.height
    );
    let mut out = vec![0; (image.width * image.height * 2) as usize];
    for y in 0..image.height {
        let row = image.height - 1 - y;
        for x in 0..image.width {
            let at = ((row / 8) * (image.width / 8) + x / 8) * 64 + morton(x % 8, row % 8);
            let texel = format.encode(image.texel(x, y));
            out[at as usize * 2..at as usize * 2 + 2].copy_from_slice(&texel.to_le_bytes());
        }
    }
    out
}

/// The inverse of `tile`, for previews: what the GPU will show.
pub fn untile(data: &[u8], width: u32, height: u32, format: Format) -> Image {
    let mut image = Image::new(width, height);
    for y in 0..height {
        let row = height - 1 - y;
        for x in 0..width {
            let at = (((row / 8) * (width / 8) + x / 8) * 64 + morton(x % 8, row % 8)) as usize * 2;
            image.set(
                x,
                y,
                format.decode(u16::from_le_bytes([data[at], data[at + 1]])),
            );
        }
    }
    image
}

/// A texel's place within its 8×8 tile: the bits of x and y interleaved, x lowest.
fn morton(x: u32, y: u32) -> u32 {
    (x & 1) | (y & 1) << 1 | (x & 2) << 1 | (y & 2) << 2 | (x & 4) << 2 | (y & 4) << 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_round_trip_their_own_precision() {
        assert_eq!(
            Format::Rgb565.decode(Format::Rgb565.encode([255, 0, 255, 255])),
            [255, 0, 255, 255]
        );
        assert_eq!(
            Format::Rgba4.decode(Format::Rgba4.encode([255, 255, 255, 0])),
            [255, 255, 255, 0]
        );
        // 4 bits: 100 rounds to 6, shown as 0x66 = 102.
        assert_eq!(
            Format::Rgba4.decode(Format::Rgba4.encode([100, 100, 100, 255]))[0],
            102
        );
    }

    #[test]
    fn tiling_round_trips() {
        let mut image = Image::new(16, 8);
        for y in 0..8 {
            for x in 0..16 {
                image.set(x, y, [x as u8 * 16, y as u8 * 32, 0, 255]);
            }
        }
        let back = untile(&tile(&image, Format::Rgb565), 16, 8, Format::Rgb565);
        for y in 0..8 {
            for x in 0..16 {
                let want = Format::Rgb565.decode(Format::Rgb565.encode(image.texel(x, y)));
                assert_eq!(back.texel(x, y), want);
            }
        }
        // The bottom row comes first: (0, 7) is texel 0.
        let first = u16::from_le_bytes(tile(&image, Format::Rgb565)[0..2].try_into().unwrap());
        assert_eq!(first, Format::Rgb565.encode(image.texel(0, 7)));
    }
}
