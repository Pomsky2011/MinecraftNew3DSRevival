//! The pack stack: packs lowest priority first, read the way Java's `FallbackResourceManager`
//! reads them (DESIGN.md §3.5).

use std::collections::BTreeSet;
use std::io;

use crate::pack::Pack;

pub struct Stack {
    /// Lowest priority first, as `options.txt` lists them.
    pub packs: Vec<Pack>,
}

/// A file found in the stack, and which pack it came from.
pub struct Found {
    pub bytes: Vec<u8>,
    /// Its index in `Stack::packs`.
    pub pack: usize,
}

impl Stack {
    /// The file at `namespace`/`path` (`path` inside `assets/<namespace>/`): from the highest pack
    /// that has it, unless a pack in between filters it, which hides it in every pack below.
    pub fn read(&self, namespace: &str, path: &str) -> io::Result<Option<Found>> {
        let file = format!("assets/{namespace}/{path}");
        for (index, pack) in self.packs.iter().enumerate().rev() {
            if let Some(bytes) = pack.read(&file)? {
                return Ok(Some(Found { bytes, pack: index }));
            }
            if pack.filters(namespace, path) {
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// A resource's `.mcmeta`, searched from the top down to the pack its file came from, and no
    /// further: a pack replacing a texture without its `.mcmeta` gets none from below.
    pub fn metadata(
        &self,
        namespace: &str,
        path: &str,
        from: usize,
    ) -> io::Result<Option<Vec<u8>>> {
        let meta = format!("{path}.mcmeta");
        let file = format!("assets/{namespace}/{meta}");
        for pack in self.packs[from..].iter().rev() {
            if let Some(bytes) = pack.read(&file)? {
                return Ok(Some(bytes));
            }
            if pack.filters(namespace, &meta) {
                break;
            }
        }
        Ok(None)
    }

    /// Every pack's copy of `namespace`/`path`, lowest first, as Java's `getResourceStack`: down
    /// from the top, ending (after its own copy) at the first pack whose filter matches. For files
    /// packs add to rather than replace, such as atlas definitions.
    pub fn read_stack(&self, namespace: &str, path: &str) -> io::Result<Vec<Found>> {
        let file = format!("assets/{namespace}/{path}");
        let mut found = Vec::new();
        for (index, pack) in self.packs.iter().enumerate().rev() {
            if let Some(bytes) = pack.read(&file)? {
                found.push(Found { bytes, pack: index });
            }
            if pack.filters(namespace, path) {
                break;
            }
        }
        found.reverse();
        Ok(found)
    }

    /// Every `(namespace, path)` under `<directory>/` in any namespace, sorted, by `list`'s rules.
    pub fn list_all(&self, directory: &str) -> io::Result<Vec<(String, String)>> {
        let mut namespaces = BTreeSet::new();
        for pack in &self.packs {
            for file in pack.files()? {
                if let Some(namespace) = file
                    .strip_prefix("assets/")
                    .and_then(|rest| rest.split('/').next())
                {
                    namespaces.insert(namespace.to_owned());
                }
            }
        }
        let mut all = Vec::new();
        for namespace in namespaces {
            for path in self.list(&namespace, directory)? {
                all.push((namespace.clone(), path));
            }
        }
        Ok(all)
    }

    /// Every path under `assets/<namespace>/<directory>/` the stack has, sorted. As Java lists: up
    /// the stack, each pack's filter first removes what the packs below listed, then its own files
    /// are added.
    pub fn list(&self, namespace: &str, directory: &str) -> io::Result<Vec<String>> {
        let prefix = format!("assets/{namespace}/{directory}/");
        let mut paths = BTreeSet::new();
        for pack in &self.packs {
            paths.retain(|path: &String| !pack.filters(namespace, path));
            for file in pack.files()? {
                if let Some(rest) = file.strip_prefix(&prefix) {
                    paths.insert(format!("{directory}/{rest}"));
                }
            }
        }
        Ok(paths.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::source::Folder;

    /// A folder pack with `pack.mcmeta` (`extra` merged into it) and files.
    fn folder_pack(root: &Path, name: &str, extra: &str, files: &[(&str, &str)]) -> Pack {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("pack.mcmeta"),
            format!(r#"{{"pack": {{"description": "", "pack_format": 46}}{extra}}}"#),
        )
        .unwrap();
        for (path, text) in files {
            let file = dir.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        Pack::open(name.into(), Box::new(Folder::new(dir))).unwrap()
    }

    fn text(found: Option<Found>) -> Option<String> {
        found.map(|f| String::from_utf8(f.bytes).unwrap())
    }

    #[test]
    fn higher_packs_win_and_filters_hide_lower_ones() {
        let dir = tempfile::tempdir().unwrap();
        let stack = Stack {
            packs: vec![
                folder_pack(
                    dir.path(),
                    "vanilla",
                    "",
                    &[
                        ("assets/minecraft/textures/block/stone.png", "vanilla stone"),
                        ("assets/minecraft/textures/block/dirt.png", "vanilla dirt"),
                        (
                            "assets/minecraft/textures/block/iron_ore.png",
                            "vanilla ore",
                        ),
                    ],
                ),
                folder_pack(
                    dir.path(),
                    "middle",
                    "",
                    &[("assets/minecraft/textures/block/stone.png", "middle stone")],
                ),
                folder_pack(
                    dir.path(),
                    "top",
                    r#", "filter": {"block": [{"path": "_ore"}]}"#,
                    &[],
                ),
            ],
        };
        assert_eq!(
            text(stack.read("minecraft", "textures/block/stone.png").unwrap()).as_deref(),
            Some("middle stone")
        );
        assert_eq!(
            text(stack.read("minecraft", "textures/block/dirt.png").unwrap()).as_deref(),
            Some("vanilla dirt")
        );
        assert!(
            stack
                .read("minecraft", "textures/block/iron_ore.png")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            stack.list("minecraft", "textures/block").unwrap(),
            ["textures/block/dirt.png", "textures/block/stone.png"]
        );
    }

    #[test]
    fn metadata_comes_only_from_the_texture_s_pack_or_above() {
        let dir = tempfile::tempdir().unwrap();
        let stack = Stack {
            packs: vec![
                folder_pack(
                    dir.path(),
                    "vanilla",
                    "",
                    &[
                        ("assets/minecraft/textures/block/water.png", "frames"),
                        (
                            "assets/minecraft/textures/block/water.png.mcmeta",
                            "animated",
                        ),
                    ],
                ),
                folder_pack(
                    dir.path(),
                    "still",
                    "",
                    &[("assets/minecraft/textures/block/water.png", "one frame")],
                ),
            ],
        };
        let found = stack
            .read("minecraft", "textures/block/water.png")
            .unwrap()
            .unwrap();
        assert_eq!(found.pack, 1);
        assert_eq!(
            stack
                .metadata("minecraft", "textures/block/water.png", found.pack)
                .unwrap(),
            None
        );
        assert!(
            stack
                .metadata("minecraft", "textures/block/water.png", 0)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn stacks_gather_every_pack_s_copy_down_to_a_filter() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = "atlases/blocks.json";
        let stack = Stack {
            packs: vec![
                folder_pack(
                    dir.path(),
                    "vanilla",
                    "",
                    &[("assets/minecraft/atlases/blocks.json", "v")],
                ),
                folder_pack(
                    dir.path(),
                    "a",
                    r#", "filter": {"block": [{"path": "atlases"}]}"#,
                    &[("assets/minecraft/atlases/blocks.json", "a")],
                ),
                folder_pack(
                    dir.path(),
                    "b",
                    "",
                    &[("assets/minecraft/atlases/blocks.json", "b")],
                ),
            ],
        };
        let found: Vec<String> = stack
            .read_stack("minecraft", atlas)
            .unwrap()
            .into_iter()
            .map(|f| String::from_utf8(f.bytes).unwrap())
            .collect();
        // "a" filters what's below it, but keeps its own copy.
        assert_eq!(found, ["a", "b"]);
    }

    #[test]
    fn overlays_override_their_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack = folder_pack(
            dir.path(),
            "p",
            r#", "overlays": {"entries": [{"directory": "now", "min_format": 75, "max_format": 75}]}"#,
            &[
                ("assets/minecraft/textures/block/stone.png", "base"),
                ("now/assets/minecraft/textures/block/stone.png", "overlay"),
            ],
        );
        let stack = Stack { packs: vec![pack] };
        assert_eq!(
            text(stack.read("minecraft", "textures/block/stone.png").unwrap()).as_deref(),
            Some("overlay")
        );
    }
}
