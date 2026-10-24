//! Resource packs and the conversion of what they hold, as Java takes them (DESIGN.md §3.5, §3.6).
//! Both `n3dsr-build` (on the PC) and the 3DS app (for packs added on the SD card) run this, so
//! it's pure Rust and knows neither.

pub mod atlas;
pub mod image;
pub mod mipmap;
pub mod options;
pub mod pack;
pub mod pica;
pub mod source;
pub mod sprite;
mod srgb_tables;
pub mod stack;
pub mod stitch;
