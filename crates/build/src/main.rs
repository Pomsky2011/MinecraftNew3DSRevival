//! `n3dsr-build`: converts the game files a proof names into the 3DS app's assets (DESIGN.md
//! §3.2–3.4). The player runs it on the proof the mod wrote.

mod icon;
mod proof;
mod release;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use n3dsr_assets::pack::Pack;
use n3dsr_assets::source::{AssetIndex, Layered, Zip};
use n3dsr_assets::stack::Stack;
use n3dsr_assets::{atlas, options};
use proof::Proof;

/// The repository, which a player builds from: converted assets go in its gitignored `assets/`.
const REPOSITORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The proof the mod wrote, e.g. ~/.config/n3dsr/proofs/1.21.11.json.
    proof: PathBuf,
}

/// The game's own pack stack (DESIGN.md §3.5): the vanilla pack (the client jar, then the asset
/// store), under the packs `options.txt` enables.
fn pack_stack(proof: &Proof) -> Result<Stack> {
    let vanilla = Layered(vec![
        Box::new(Zip::open(&proof.client_jar.path)?),
        Box::new(AssetIndex::open(
            &proof.assets.directory,
            &proof.assets.index,
        )?),
    ]);
    let vanilla = Pack::built_in("vanilla".into(), Box::new(vanilla));
    let options = fs::read_to_string(proof.game_directory.join("options.txt")).unwrap_or_default();
    let enabled = options::enabled_packs(&options);
    let (stack, notes) = options::stack(
        &enabled,
        vanilla,
        &proof.game_directory.join("resourcepacks"),
    );
    println!("Resource packs, lowest first:");
    for pack in &stack.packs {
        println!("  {}", pack.id);
    }
    for note in notes {
        println!("  note: {}: {}", note.pack, note.note);
    }
    Ok(stack)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let proof = Proof::read(&cli.proof)?;
    release::check(&proof.release)?;

    println!("Checking the game files against the proof…");
    proof.verify()?;

    let stack = pack_stack(&proof)?;

    let repository = Path::new(REPOSITORY).canonicalize()?;
    let out = repository.join("assets").join(&proof.release);
    fs::create_dir_all(&out)?;

    let texture = stack
        .read("minecraft", icon::TEXTURE)?
        .with_context(|| format!("no pack has {}", icon::TEXTURE))?;
    let texture = icon::Image::decode_png(texture.bytes.as_slice())?;
    let (large, small) = icon::icons(&texture)?;
    large.write_png(&out.join("icon.png"))?;
    small.write_png(&out.join("icon-small.png"))?;
    // cargo-3ds uses the client crate's icon.png when there is one (it can't take a small icon;
    // step 2h, building the app here, will pass both).
    let client_icon = repository.join("crates/client/icon.png");
    fs::copy(out.join("icon.png"), &client_icon)?;
    println!(
        "Icons: {} (and icon-small.png), {}",
        out.join("icon.png").display(),
        client_icon.display()
    );

    println!("Atlases:");
    for definition in stack.list("minecraft", "atlases")? {
        let Some(name) = definition
            .strip_prefix("atlases/")
            .and_then(|n| n.strip_suffix(".json"))
        else {
            continue;
        };
        let atlas = atlas::load(&stack, name)?;
        let animated = atlas
            .sprites
            .iter()
            .filter(|s| s.animation.is_some())
            .count();
        let interpolated = atlas
            .sprites
            .iter()
            .filter(|s| s.animation.as_ref().is_some_and(|a| a.interpolate))
            .count();
        let frames: u32 = atlas
            .sprites
            .iter()
            .map(|s| s.animation.as_ref().map_or(1, |_| s.frame_count()))
            .sum();
        println!(
            "  {name}: {} sprites, {frames} frames ({animated} animated, {interpolated} interpolated)",
            atlas.sprites.len()
        );
        for note in &atlas.notes {
            println!("    note: {note}");
        }
    }
    Ok(())
}
