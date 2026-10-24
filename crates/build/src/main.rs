//! `n3dsr-build`: downloads a release of Minecraft: Java Edition and converts it for the 3DS
//! (DESIGN.md §3).

mod cache;
mod mojang;
mod notice;
mod release;

use std::fs;

use anyhow::Result;
use clap::{Parser, Subcommand};

use cache::{Cache, Expected};
use mojang::{AssetIndex, Manifest, Version};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download a release's client jar and asset index into the cache.
    Fetch {
        /// The release, e.g. 1.21.11.
        release: String,
        /// Confirm you own Minecraft: Java Edition without being asked.
        #[arg(long)]
        i_own_minecraft: bool,
    },
    /// List the releases this source can build.
    Releases,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Fetch {
            release,
            i_own_minecraft,
        } => {
            notice::confirm(i_own_minecraft)?;
            fetch(&Cache::new()?, &release)
        }
        Command::Releases => {
            for id in release::SUPPORTED {
                println!("{id}\t(feature {})", release::feature(id));
            }
            Ok(())
        }
    }
}

fn fetch(cache: &Cache, id: &str) -> Result<()> {
    let manifest = cache.get_fresh("version_manifest_v2.json", mojang::MANIFEST_URL)?;
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest)?)?;
    let entry = release::choose(&manifest, id)?;

    let version = cache.get(
        &format!("versions/{id}/{id}.json"),
        &Expected {
            url: &entry.url,
            sha1: &entry.sha1,
            size: None,
        },
    )?;
    let version: Version = serde_json::from_slice(&fs::read(version)?)?;

    let client = &version.downloads.client;
    println!("client jar ({:.1} MB)…", client.size as f64 / 1e6);
    let jar = cache.get(
        &format!("versions/{id}/client.jar"),
        &Expected {
            url: &client.url,
            sha1: &client.sha1,
            size: Some(client.size),
        },
    )?;

    let reference = &version.asset_index;
    let index = cache.get(
        &format!("assets/indexes/{}.json", reference.id),
        &Expected {
            url: &reference.url,
            sha1: &reference.sha1,
            size: Some(reference.size),
        },
    )?;
    let assets: AssetIndex = serde_json::from_slice(&fs::read(&index)?)?;

    println!("{}: {}", version.id, jar.display());
    println!(
        "asset index {}: {} objects, {}",
        reference.id,
        assets.objects.len(),
        index.display()
    );
    Ok(())
}
