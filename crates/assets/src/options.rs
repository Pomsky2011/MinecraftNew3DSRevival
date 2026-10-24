//! The pack stack the game itself uses, from `options.txt`'s `resourcePacks` (DESIGN.md §3.5), by
//! Java's rules (`Options.loadSelectedResourcePacks`, `ClientPackSource`).

use std::path::Path;

use crate::pack::{Compatibility, Pack};
use crate::source::{Folder, Source, Zip};
use crate::stack::Stack;

/// Something about a pack the player should hear: one left out, or loaded though incompatible.
#[derive(Debug, PartialEq)]
pub struct Note {
    pub pack: String,
    pub note: String,
}

/// `options.txt`'s `resourcePacks`, lowest priority first; empty if there's none.
pub fn enabled_packs(options: &str) -> Vec<String> {
    options
        .lines()
        .find_map(|line| line.strip_prefix("resourcePacks:"))
        .and_then(|list| serde_json::from_str(list).ok())
        .unwrap_or_default()
}

/// The stack for `enabled` (lowest first): `vanilla` from `vanilla`, `file/<name>` from
/// `resource_packs`, built-ins from inside the vanilla pack. Packs mods provide can't be built
/// without the mods, so they're left out, with a note.
pub fn stack(enabled: &[String], vanilla: Pack, resource_packs: &Path) -> (Stack, Vec<Note>) {
    let mut notes = Vec::new();
    let mut note = |pack: &str, note: String| {
        notes.push(Note {
            pack: pack.to_owned(),
            note,
        })
    };

    let mut packs = Vec::new();
    let mut vanilla = Some(vanilla);
    for id in enabled {
        if id == "vanilla" {
            if let Some(vanilla) = vanilla.take() {
                packs.push(vanilla);
            }
            continue;
        }
        let opened = if let Some(name) = id.strip_prefix("file/") {
            open_file_pack(id, resource_packs, name)
        } else if let Some(built_in) = vanilla
            .as_ref()
            .or_else(|| packs.iter().find(|p| p.id == "vanilla"))
            .and_then(|v| built_in_pack(v, id))
        {
            Some(built_in)
        } else if id.contains(':') || id == "fabric" {
            note(
                id,
                "provided by a mod, so it can't be built; left out".into(),
            );
            continue;
        } else {
            // As Java: a bare name not found is tried as a file pack.
            open_file_pack(&format!("file/{id}"), resource_packs, id)
        };
        match opened {
            Some(Ok(pack)) => {
                if pack.formats.compatibility(crate::pack::CURRENT) != Compatibility::Compatible {
                    note(
                        id,
                        format!(
                            "made for formats {:?}, so it may look wrong; loaded anyway, as the game does",
                            pack.formats
                        ),
                    );
                }
                packs.push(pack);
            }
            Some(Err(reason)) => note(
                id,
                format!("the game can't load it either ({reason}); left out"),
            ),
            None => note(id, "not found; left out".into()),
        }
    }
    // The vanilla pack is required: at the bottom if the list leaves it out.
    if let Some(vanilla) = vanilla {
        packs.insert(0, vanilla);
    }
    (Stack { packs }, notes)
}

/// A pack in `resourcepacks/`: a zip or a folder.
fn open_file_pack(id: &str, resource_packs: &Path, name: &str) -> Option<Result<Pack, String>> {
    if name.contains('/') || name.contains('\\') || name == ".." {
        return None;
    }
    let path = resource_packs.join(name);
    let source: Box<dyn Source> = if path.is_dir() {
        Box::new(Folder::new(path))
    } else if path.is_file() {
        match Zip::open(&path) {
            Ok(zip) => Box::new(zip),
            Err(e) => return Some(Err(format!("not a zip: {e}"))),
        }
    } else {
        return None;
    };
    Some(Pack::open(id.to_owned(), source))
}

/// A built-in pack (`programmer_art`, `high_contrast`): a zip in the vanilla pack's
/// `resourcepacks/`.
fn built_in_pack(vanilla: &Pack, id: &str) -> Option<Result<Pack, String>> {
    let bytes = vanilla
        .read(&format!("assets/minecraft/resourcepacks/{id}.zip"))
        .ok()??;
    Some(
        Zip::from_bytes(bytes)
            .map_err(|e| format!("not a zip: {e}"))
            .and_then(|zip| Pack::open(id.to_owned(), Box::new(zip))),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::source::tests::zip_with;

    #[test]
    fn reads_the_list() {
        let options = "version:4671\nresourcePacks:[\"vanilla\",\"file/A.zip\"]\nlang:en_us\n";
        assert_eq!(enabled_packs(options), ["vanilla", "file/A.zip"]);
        assert!(enabled_packs("lang:en_us").is_empty());
    }

    #[test]
    fn builds_the_stack_as_the_game_does() {
        let dir = tempfile::tempdir().unwrap();
        let meta = br#"{"pack": {"description": "", "min_format": 75, "max_format": 75}}"#;
        // The vanilla pack, holding a built-in pack's zip.
        let built_in = dir.path().join("hc.zip");
        zip_with(&built_in, &[("pack.mcmeta", meta)]);
        let vanilla_zip = dir.path().join("vanilla.zip");
        zip_with(
            &vanilla_zip,
            &[(
                "assets/minecraft/resourcepacks/high_contrast.zip",
                &fs::read(&built_in).unwrap(),
            )],
        );
        let vanilla = Pack::built_in("vanilla".into(), Box::new(Zip::open(&vanilla_zip).unwrap()));
        let packs = dir.path().join("resourcepacks");
        fs::create_dir(&packs).unwrap();
        zip_with(&packs.join("A.zip"), &[("pack.mcmeta", meta)]);
        zip_with(
            &packs.join("Old.zip"),
            &[(
                "pack.mcmeta",
                br#"{"pack": {"description": "", "pack_format": 46}}"#,
            )],
        );

        let enabled: Vec<String> = [
            "vanilla",
            "file/A.zip",
            "continuity:default",
            "high_contrast",
            "Old.zip",
            "file/Gone.zip",
        ]
        .map(String::from)
        .to_vec();
        let (stack, notes) = stack(&enabled, vanilla, &packs);
        let ids: Vec<&str> = stack.packs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            ["vanilla", "file/A.zip", "high_contrast", "file/Old.zip"]
        );
        let noted: Vec<&str> = notes.iter().map(|n| n.pack.as_str()).collect();
        assert_eq!(noted, ["continuity:default", "Old.zip", "file/Gone.zip"]);
    }
}
