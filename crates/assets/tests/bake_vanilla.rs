//! Our baked block models against the game's own (DESIGN.md §3.7, §11.2), every quad, floats as
//! raw bits: vectors written by `mod/`'s `./gradlew blockModelVectors` from the player's client
//! jar into the gitignored `assets/`. Without them there's nothing to compare, and the test says so.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;

use n3dsr_assets::bake::{ModelState, Models, NoSprites, Octahedral, Quad};
use n3dsr_assets::model::{Direction, Model, Quadrant};

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/1.21.11/vectors/block_models.txt"
);

fn name(direction: Direction) -> &'static str {
    match direction {
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
    }
}

/// A quad as the harness writes it; UVs through Java's `getU` on a whole-atlas sprite (`0 + 1 × u`).
fn line(quad: &Quad) -> String {
    let mut out = format!(
        "{} {} {} {} {} {}",
        quad.texture.as_deref().unwrap_or("-"),
        quad.tint,
        name(quad.facing),
        quad.cull.map_or("-", name),
        quad.shade,
        quad.light_emission
    );
    for p in &quad.positions {
        for c in p {
            out += &format!(" {:08x}", c.to_bits());
        }
    }
    for uv in &quad.uvs {
        for c in uv {
            out += &format!(" {:08x}", (0.0f32 + 1.0 * c).to_bits());
        }
    }
    out
}

#[test]
fn matches_the_game_s_baking() {
    let Ok(text) = fs::read_to_string(VECTORS) else {
        eprintln!("no vectors at {VECTORS}: run ./gradlew blockModelVectors in mod/ to compare");
        return;
    };
    let mut lines = text.lines();
    let jar = lines.next().unwrap().strip_prefix("jar ").unwrap();

    let mut zip = zip::ZipArchive::new(File::open(jar).unwrap()).unwrap();
    let mut files = HashMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let path = entry.name().unwrap().into_owned();
        let Some(rest) = path
            .strip_prefix("assets/minecraft/models/")
            .and_then(|p| p.strip_suffix(".json"))
        else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.insert(format!("minecraft:{rest}"), Model::parse(&bytes).unwrap());
    }
    let mut models = Models::new(files);
    let mut states: HashMap<(u8, u8, u8, bool), ModelState> = HashMap::new();

    let (mut orientations, mut quads, mut mismatches) = (0, 0, Vec::new());
    let mut lines = lines.peekable();
    while let Some(header) = lines.next() {
        let fields: Vec<&str> = header.split(' ').collect();
        let [_, model, x, y, z, uvlock, ao, count] = fields[..] else {
            panic!("bad header {header}")
        };
        let (x, y, z) = (x.parse().unwrap(), y.parse().unwrap(), z.parse().unwrap());
        let uvlock: bool = uvlock.parse().unwrap();
        let state = states.entry((x, y, z, uvlock)).or_insert_with(|| {
            ModelState::new(
                Octahedral::from_xyz(Quadrant(x), Quadrant(y), Quadrant(z)),
                uvlock,
            )
        });
        let baked = models.bake(model, state, &NoSprites);
        let theirs: Vec<&str> = (0..count.parse::<usize>().unwrap())
            .map(|_| lines.next().unwrap())
            .collect();
        let ours: Vec<String> = baked.quads.iter().map(line).collect();
        orientations += 1;
        quads += theirs.len();
        if baked.ambient_occlusion.to_string() != ao {
            mismatches.push(format!(
                "{header}: ambient occlusion {}",
                baked.ambient_occlusion
            ));
        }
        if ours.len() != theirs.len() {
            mismatches.push(format!(
                "{header}: {} quads, theirs {}",
                ours.len(),
                theirs.len()
            ));
            continue;
        }
        for (i, (mine, theirs)) in ours.iter().zip(&theirs).enumerate() {
            if mine != theirs {
                mismatches.push(format!(
                    "{header} quad {i}:\n  ours   {mine}\n  theirs {theirs}"
                ));
            }
        }
    }
    assert!(orientations > 0);
    let shown: Vec<&String> = mismatches.iter().take(12).collect();
    assert!(
        mismatches.is_empty(),
        "{} mismatches over {orientations} orientations, {quads} quads; first:\n{}",
        mismatches.len(),
        shown
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!("{orientations} model orientations, {quads} quads identical to the game's");
}
