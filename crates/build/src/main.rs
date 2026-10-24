//! `n3dsr-build`: converts the game files a proof names into the 3DS app's assets (DESIGN.md
//! §3.2–3.4). The player runs it on the proof the mod wrote.

mod icon;
mod proof;
mod release;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use proof::Proof;

/// The repository, which a player builds from: converted assets go in its gitignored `assets/`.
const REPOSITORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The proof the mod wrote, e.g. ~/.config/n3dsr/proofs/1.21.11.json.
    proof: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let proof = Proof::read(&cli.proof)?;
    release::check(&proof.release)?;

    println!("Checking the game files against the proof…");
    proof.verify()?;

    let repository = Path::new(REPOSITORY).canonicalize()?;
    let out = repository.join("assets").join(&proof.release);
    fs::create_dir_all(&out)?;
    let mut jar = zip::ZipArchive::new(fs::File::open(&proof.client_jar.path)?)?;

    let texture = icon::Image::decode_png(
        jar.by_name(icon::TEXTURE)
            .with_context(|| format!("{} isn't in the client jar", icon::TEXTURE))?,
    )?;
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
    Ok(())
}
