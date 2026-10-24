//! `n3dsr-build`: converts the game files a proof names into the 3DS app's assets (DESIGN.md
//! §3.2–3.4). The player runs it on the proof the mod wrote.

mod icon;
mod proof;
mod release;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use n3dsr_assets::pack::Pack;
use n3dsr_assets::source::{AssetIndex, Layered, Zip};
use n3dsr_assets::stack::Stack;
use n3dsr_assets::{atlas, model, options, pica, standalone, stitch};
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

/// Every atlas the game builds, packed into GPU pages with their index, and previews of each page
/// (DESIGN.md §3.6).
fn atlases(stack: &Stack, out: &Path) -> Result<BTreeSet<(String, String)>> {
    let mut in_atlases = BTreeSet::new();
    let romfs = out.join("romfs/atlases");
    let preview = out.join("preview");
    fs::create_dir_all(&romfs)?;
    fs::create_dir_all(&preview)?;
    println!("Atlases:");
    for &(name, mipmaps) in atlas::KNOWN {
        let atlas = atlas::load(stack, name)?;
        in_atlases.extend(atlas.files.iter().cloned());
        let wanted = if mipmaps { MIP_LEVELS } else { 0 };
        let stitched = stitch::stitch(&atlas.sprites, wanted);
        let pages: Vec<String> = stitched
            .pages
            .iter()
            .map(|p| format!("{}×{} {:?}", p.width(), p.height(), p.format))
            .collect();
        println!(
            "  {name}: {} sprites, {} mip levels, pages {}",
            atlas.sprites.len(),
            stitched.levels,
            pages.join(", ")
        );
        for note in &atlas.notes {
            println!("    note: {note}");
        }
        let limiting = stitch::limiting_sprites(&atlas.sprites, wanted);
        if !limiting.is_empty() {
            let named: Vec<&str> = limiting.iter().take(3).map(|(id, _)| *id).collect();
            println!(
                "    note: {} sprite(s) limit the mip levels from {wanted} to {}, as in the game: {}{}",
                limiting.len(),
                stitched.levels,
                named.join(", "),
                if limiting.len() > 3 { ", …" } else { "" }
            );
        }
        for (index, page) in stitched.pages.iter().enumerate() {
            fs::write(romfs.join(format!("{name}-{index}.bin")), page.to_bytes())?;
            page.levels[0].write_png(&preview.join(format!("{name}-{index}.png")))?;
            let gpu = page.to_bytes();
            let on_3ds = pica::untile(&gpu[10..], page.width(), page.height(), page.format);
            on_3ds.write_png(&preview.join(format!("{name}-{index}-3ds.png")))?;
        }
        let index = serde_json::json!({
            "levels": stitched.levels,
            "pages": stitched.pages.iter().map(|p| serde_json::json!({
                "format": p.format, "width": p.width(), "height": p.height(), "levels": p.levels.len(),
            })).collect::<Vec<_>>(),
            "sprites": stitched.sprites,
        });
        fs::write(
            romfs.join(format!("{name}.json")),
            serde_json::to_vec(&index)?,
        )?;
    }
    println!("Previews: {}", preview.display());
    Ok(in_atlases)
}

/// Every texture no atlas took (but those the build leaves out), as GPU textures of their own,
/// with an index; and the colour maps as data too (DESIGN.md §3.6).
fn standalone(stack: &Stack, out: &Path, in_atlases: &BTreeSet<(String, String)>) -> Result<()> {
    let romfs = out.join("romfs");
    let rows: BTreeMap<(String, String), u32> =
        standalone::font_row_heights(stack)?.into_iter().collect();
    let mut index = serde_json::Map::new();
    let (mut count, mut bytes) = (0, 0);
    for (namespace, path) in stack.list_all("textures")? {
        let key = (namespace.clone(), path.clone());
        if !path.ends_with(".png")
            || in_atlases.contains(&key)
            || standalone::left_out(&namespace, &path)
        {
            continue;
        }
        let texture = match standalone::load(stack, &namespace, &path, rows.get(&key).copied()) {
            Ok(texture) => texture,
            Err(note) => {
                println!("  note: {note}");
                continue;
            }
        };
        // romfs/textures/<namespace>/<path without textures/ and .png>[-<strip>].bin
        let stem = romfs
            .join("textures")
            .join(&namespace)
            .join(&path["textures/".len()..path.len() - ".png".len()]);
        fs::create_dir_all(stem.parent().unwrap())?;
        for (strip, page) in texture.pages.iter().enumerate() {
            let file = if texture.pages.len() == 1 {
                stem.with_extension("bin")
            } else {
                stem.with_file_name(format!(
                    "{}-{strip}.bin",
                    stem.file_name().unwrap().to_string_lossy()
                ))
            };
            let data = page.to_bytes();
            bytes += data.len();
            fs::write(file, data)?;
        }
        if let Some(name) = path.strip_prefix(standalone::COLORMAP_DIR) {
            let raw = romfs
                .join("colormap")
                .join(&namespace)
                .join(name)
                .with_extension("rgb");
            fs::create_dir_all(raw.parent().unwrap())?;
            let image =
                texture.pages[0].levels[0].crop(0, 0, texture.entry.width, texture.entry.height);
            fs::write(raw, standalone::colormap_rgb(&image))?;
        }
        index.insert(texture.id, serde_json::to_value(texture.entry)?);
        count += 1;
    }
    fs::write(romfs.join("textures.json"), serde_json::to_vec(&index)?)?;
    println!("Standalone textures: {count}, {:.1} MB", bytes as f64 / 1e6);
    Ok(())
}

/// Every blockstate and model file, parsed (baking comes next).
fn models(stack: &Stack) -> Result<()> {
    let (mut states, mut models) = (0, 0);
    for (namespace, path) in stack.list_all("blockstates")? {
        let file = stack.read(&namespace, &path)?.unwrap();
        match model::BlockState::parse(&file.bytes) {
            Ok(_) => states += 1,
            Err(e) => println!("  note: {namespace}:{path}: {e}"),
        }
    }
    for (namespace, path) in stack.list_all("models")? {
        if !path.ends_with(".json") {
            continue;
        }
        let file = stack.read(&namespace, &path)?.unwrap();
        match model::Model::parse(&file.bytes) {
            Ok(_) => models += 1,
            Err(e) => println!("  note: {namespace}:{path}: {e}"),
        }
    }
    println!("Parsed {states} blockstates and {models} models");
    Ok(())
}

/// Mip levels for atlases that have them: Java's most (its Mipmap Levels option's top), which
/// the 3DS can use fewer of.
const MIP_LEVELS: u32 = 4;

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

    let in_atlases = atlases(&stack, &out)?;
    standalone(&stack, &out, &in_atlases)?;
    models(&stack)?;
    Ok(())
}
