//! Our baked item models against the game's own (DESIGN.md §3.7, §11.2), every quad, floats as
//! raw bits: vectors written by `mod/`'s `./gradlew itemModelVectors` from the player's client jar
//! into the gitignored `assets/`. The sprites come from our own atlas loading of the same jar, so
//! this checks that too. Generated models' quads are compared as sets: Java's order for them is
//! random (hash sets), different each run.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use n3dsr_assets::atlas;
use n3dsr_assets::bake::{ModelState, Models, Octahedral, Quad, Sprites};
use n3dsr_assets::model::{Direction, Model};
use n3dsr_assets::pack::Pack;
use n3dsr_assets::source::Zip;
use n3dsr_assets::sprite::Sprite;
use n3dsr_assets::stack::Stack;

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/1.21.11/vectors/item_models.txt"
);

/// `block_or_item`: the items atlas, then the blocks atlas.
struct BlockOrItem {
    items: HashMap<String, Sprite>,
    blocks: HashMap<String, Sprite>,
}

impl Sprites for BlockOrItem {
    fn sprite(&self, texture: &str) -> Option<&Sprite> {
        self.items.get(texture).or_else(|| self.blocks.get(texture))
    }
}

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

/// As the harness writes a quad; a texture that's in neither atlas is the missing sprite there.
fn line(quad: &Quad, sprites: &BlockOrItem) -> String {
    let texture = match quad.texture.as_deref() {
        None => "-",
        Some(t) if sprites.sprite(t).is_some() => t,
        Some(_) => "minecraft:missingno",
    };
    let mut out = format!(
        "{texture} {} {} {} {} {}",
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
fn matches_the_game_s_item_models() {
    let Ok(text) = fs::read_to_string(VECTORS) else {
        eprintln!("no vectors at {VECTORS}: run ./gradlew itemModelVectors in mod/ to compare");
        return;
    };
    let mut lines = text.lines();
    let jar = lines.next().unwrap().strip_prefix("jar ").unwrap();

    let stack = Stack {
        packs: vec![Pack::built_in(
            "vanilla".into(),
            Box::new(Zip::open(Path::new(jar)).unwrap()),
        )],
    };
    let by_id = |name: &str| -> HashMap<String, Sprite> {
        atlas::load(&stack, name)
            .unwrap()
            .sprites
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };
    let sprites = BlockOrItem {
        items: by_id("items"),
        blocks: by_id("blocks"),
    };

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
    let identity = ModelState::new(Octahedral::IDENTITY, false);

    let (mut count, mut quads, mut generated, mut mismatches) = (0, 0, 0, Vec::new());
    while let Some(header) = lines.next() {
        let fields: Vec<&str> = header.split(' ').collect();
        let [_, model, ao, n] = fields[..] else {
            panic!("bad header {header}")
        };
        let theirs: Vec<String> = (0..n.parse::<usize>().unwrap())
            .map(|_| lines.next().unwrap().to_owned())
            .collect();
        let baked = models.bake(model, &identity, &sprites);
        let mut ours: Vec<String> = baked.quads.iter().map(|q| line(q, &sprites)).collect();
        let mut theirs = theirs;
        if models.is_generated(model) {
            ours.sort();
            theirs.sort();
            generated += 1;
        }
        count += 1;
        quads += theirs.len();
        if baked.ambient_occlusion.to_string() != ao {
            mismatches.push(format!(
                "{header}: ambient occlusion {}",
                baked.ambient_occlusion
            ));
        }
        if ours != theirs {
            let first = ours.iter().zip(&theirs).position(|(a, b)| a != b);
            mismatches.push(match first {
                Some(i) => format!(
                    "{header} quad {i}:\n  ours   {}\n  theirs {}",
                    ours[i], theirs[i]
                ),
                None => format!("{header}: {} quads, theirs {}", ours.len(), theirs.len()),
            });
        }
    }
    assert!(count > 0);
    assert!(
        mismatches.is_empty(),
        "{} of {count} item models differ ({quads} quads); first:\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!("{count} item models ({generated} generated), {quads} quads identical to the game's");
}
