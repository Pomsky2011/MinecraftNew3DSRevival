//! Our mip levels against the game's own (DESIGN.md §11.2), pixel for pixel: vectors written by
//! `mod/`'s `./gradlew mipmapVectors` from the player's client jar into the gitignored `assets/`.
//! Without them (on a machine with no proof, say) there's nothing to compare, and the test says so.

use std::fs::{self, File};
use std::path::Path;

use n3dsr_assets::image::Image;
use n3dsr_assets::mipmap;
use n3dsr_assets::sprite::{MipmapStrategy, Sprite};

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/1.21.11/vectors/mipmap"
);
const LEVELS: u32 = 4;

#[test]
fn matches_the_game_s_mipmapping() {
    let root = Path::new(VECTORS);
    if !root.is_dir() {
        eprintln!("no vectors at {VECTORS}: run ./gradlew mipmapVectors in mod/ to compare");
        return;
    }
    let mut compared = 0;
    let mut mismatches = Vec::new();
    for namespace_dir in fs::read_dir(root).unwrap() {
        let kind = namespace_dir.unwrap().path(); // block/, item/
        for sprite_dir in fs::read_dir(&kind).unwrap() {
            let sprite_dir = sprite_dir.unwrap().path();
            let path = format!(
                "{}/{}",
                kind.file_name().unwrap().to_str().unwrap(),
                sprite_dir.file_name().unwrap().to_str().unwrap()
            );
            let input =
                Image::decode_png(File::open(sprite_dir.join("input.png")).unwrap()).unwrap();
            for (name, strategy) in [
                ("auto", MipmapStrategy::Auto),
                ("mean", MipmapStrategy::Mean),
                ("cutout", MipmapStrategy::Cutout),
                ("strict_cutout", MipmapStrategy::StrictCutout),
                ("dark_cutout", MipmapStrategy::DarkCutout),
            ] {
                let mut sprite = Sprite::still(format!("minecraft:{path}"), input.clone());
                sprite.mipmap_strategy = strategy;
                let ours = mipmap::levels(&sprite, LEVELS);
                for (level, mine) in ours.iter().enumerate() {
                    let theirs = Image::decode_png(
                        File::open(sprite_dir.join(name).join(format!("{level}.png"))).unwrap(),
                    )
                    .unwrap();
                    compared += 1;
                    if *mine != theirs {
                        let differing = mine
                            .rgba
                            .chunks(4)
                            .zip(theirs.rgba.chunks(4))
                            .filter(|(a, b)| a != b)
                            .count();
                        let first = mine
                            .rgba
                            .chunks(4)
                            .zip(theirs.rgba.chunks(4))
                            .position(|(a, b)| a != b);
                        mismatches.push(format!(
                            "{path} {name} level {level}: {differing} pixels differ (first at {first:?}: ours {:?}, theirs {:?})",
                            first.map(|i| &mine.rgba[i * 4..i * 4 + 4]),
                            first.map(|i| &theirs.rgba[i * 4..i * 4 + 4])
                        ));
                    }
                }
            }
        }
    }
    assert!(compared > 0);
    assert!(
        mismatches.is_empty(),
        "{} of {compared} levels differ:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    eprintln!("{compared} levels identical to the game's");
}
