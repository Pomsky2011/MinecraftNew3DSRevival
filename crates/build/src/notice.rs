//! The ownership notice (DESIGN.md §2.2, §3.1): shown before the first download, confirmed once.

use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};

const NOTICE: &str = "\
n3dsr-build downloads Minecraft: Java Edition from Mojang and converts it for your 3DS.
It is for people who own Minecraft: Java Edition. Mojang's code and assets aren't part of this
project; what the tool makes is for your own console and mustn't be shared.";

/// Returns once the player has confirmed they own the game, now or on an earlier run.
pub fn confirm(flag: bool) -> Result<()> {
    let config = dirs::config_dir().context("no config directory on this system")?;
    let marker = config.join("n3dsr").join("owns-minecraft");
    if marker.exists() {
        return Ok(());
    }

    println!("{NOTICE}\n");
    if !flag {
        if !io::stdin().is_terminal() {
            bail!("not confirmed: run interactively, or pass --i-own-minecraft");
        }
        print!("Do you own Minecraft: Java Edition? Type \"yes\": ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if answer.trim() != "yes" {
            bail!("not confirmed");
        }
    }
    fs::create_dir_all(marker.parent().unwrap())?;
    fs::write(&marker, "confirmed\n")?;
    println!(
        "Confirmed; you won't be asked again ({}).\n",
        marker.display()
    );
    Ok(())
}
