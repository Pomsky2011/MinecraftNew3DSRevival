//! Java's random number generators, reimplemented bit for bit (DESIGN.md §5.2): world generation
//! and many behaviours depend on their exact sequences.

mod java;

pub use java::JavaRandom;
