//! Textures in the layout the PICA200 reads: 8×8 tiles, Morton order inside each tile, rows from
//! the bottom up. The build tool will do this on the PC for the real atlases (DESIGN.md §3.3);
//! for now the client does it at startup.

/// The test texture: 16×16, made here rather than taken from Minecraft (DESIGN.md §2.1). Two
/// greens in 4×4 checks, a red top row and a blue left column, so a texture drawn upside down or
/// mirrored shows at a glance.
pub fn test_pattern() -> (u16, u16, Vec<u8>) {
    const SIZE: usize = 16;
    let mut rgba = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let texel = if y == 0 {
                [0xD0, 0x20, 0x20, 0xFF]
            } else if x == 0 {
                [0x20, 0x40, 0xD0, 0xFF]
            } else if (x / 4 + y / 4) % 2 == 0 {
                [0x5A, 0x9E, 0x3A, 0xFF]
            } else {
                [0x3C, 0x74, 0x26, 0xFF]
            };
            rgba.extend_from_slice(&texel);
        }
    }
    (SIZE as u16, SIZE as u16, tile_rgba8(SIZE, SIZE, &rgba))
}

/// Converts row-major RGBA8 (top row first) to the GPU's tiled RGBA8. Width and height must be
/// multiples of 8, as the GPU's sizes (powers of two from 8) always are.
pub fn tile_rgba8(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    assert!(
        width.is_multiple_of(8) && height.is_multiple_of(8),
        "{width}×{height} isn't whole tiles"
    );
    assert_eq!(rgba.len(), width * height * 4);
    let mut tiled = vec![0; rgba.len()];
    for y in 0..height {
        // The GPU's first row is the bottom one.
        let row = height - 1 - y;
        for x in 0..width {
            let tile = (row / 8) * (width / 8) + x / 8;
            let texel = tile * 64 + morton(x % 8, row % 8);
            let [r, g, b, a] = rgba[(y * width + x) * 4..][..4] else {
                unreachable!()
            };
            // An RGBA8 texel is the little-endian word 0xRRGGBBAA, so A comes first in memory.
            tiled[texel * 4..][..4].copy_from_slice(&[a, b, g, r]);
        }
    }
    tiled
}

/// A texel's place within its 8×8 tile: the bits of x and y interleaved, x lowest.
fn morton(x: usize, y: usize) -> usize {
    (x & 1) | (y & 1) << 1 | (x & 2) << 1 | (y & 2) << 2 | (x & 4) << 2 | (y & 4) << 3
}
