//! Where a pack's files come from: a zip (a resource pack, or the client jar), a folder, or the
//! launcher's asset index. Each is read as a tree of `/`-separated paths, as inside a zip:
//! `pack.mcmeta`, `assets/minecraft/textures/block/stone.png`.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;

pub trait Source: Send + Sync {
    /// The file at `path`, or `None` if there's none.
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>>;
    /// Every file's path, sorted.
    fn files(&self) -> io::Result<Vec<String>>;
}

/// A zip: a resource pack, or the client jar; from a file, or from memory (a built-in pack held
/// in the asset store).
pub struct Zip<R = File> {
    archive: Mutex<zip::ZipArchive<R>>,
}

impl Zip {
    pub fn open(path: &Path) -> io::Result<Self> {
        let archive = zip::ZipArchive::new(File::open(path)?).map_err(io::Error::other)?;
        Ok(Self {
            archive: Mutex::new(archive),
        })
    }
}

impl Zip<io::Cursor<Vec<u8>>> {
    pub fn from_bytes(bytes: Vec<u8>) -> io::Result<Self> {
        let archive = zip::ZipArchive::new(io::Cursor::new(bytes)).map_err(io::Error::other)?;
        Ok(Self {
            archive: Mutex::new(archive),
        })
    }
}

impl<R: Read + io::Seek + Send> Source for Zip<R> {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        let mut archive = self.archive.lock().unwrap();
        let mut entry = match archive.by_name(path) {
            Ok(entry) if entry.is_file() => entry,
            Ok(_) | Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(io::Error::other(e)),
        };
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }

    fn files(&self) -> io::Result<Vec<String>> {
        let archive = self.archive.lock().unwrap();
        let mut files: Vec<String> = archive
            .file_names()
            .filter_map(Result::ok)
            .filter(|name| !name.ends_with('/'))
            .map(|name| name.into_owned())
            .collect();
        files.sort();
        Ok(files)
    }
}

/// A pack unzipped into a folder.
pub struct Folder {
    root: PathBuf,
}

impl Folder {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Source for Folder {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        // Paths come from packs; one climbing out of the pack is no file of it.
        if path.split('/').any(|part| part == ".." || part.is_empty()) {
            return Ok(None);
        }
        match fs::read(self.root.join(path)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::IsADirectory
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn files(&self) -> io::Result<Vec<String>> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    walk(root, &path, out)?;
                } else if let Ok(relative) = path.strip_prefix(root) {
                    let parts: Option<Vec<&str>> = relative.iter().map(|p| p.to_str()).collect();
                    if let Some(parts) = parts {
                        out.push(parts.join("/"));
                    }
                }
            }
            Ok(())
        }
        let mut files = Vec::new();
        walk(&self.root, &self.root, &mut files)?;
        files.sort();
        Ok(files)
    }
}

/// The launcher's asset store, by its index: the vanilla pack's files that aren't in the jar
/// (sounds, most language files, the built-in packs). Index keys (`minecraft/sounds/x.ogg`) are
/// read as `assets/minecraft/sounds/x.ogg`.
pub struct AssetIndex {
    objects: PathBuf,
    by_path: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct IndexFile {
    objects: BTreeMap<String, IndexObject>,
}

#[derive(Deserialize)]
struct IndexObject {
    hash: String,
}

impl AssetIndex {
    /// `directory` holds `indexes/<index>.json` and `objects/`.
    pub fn open(directory: &Path, index: &str) -> io::Result<Self> {
        let file = fs::read(directory.join("indexes").join(format!("{index}.json")))?;
        let parsed: IndexFile = serde_json::from_slice(&file).map_err(io::Error::other)?;
        let by_path = parsed
            .objects
            .into_iter()
            .map(|(key, object)| (format!("assets/{key}"), object.hash))
            .collect();
        Ok(Self {
            objects: directory.join("objects"),
            by_path,
        })
    }

    /// Where an object's file is, by its hash.
    fn object_path(&self, hash: &str) -> Option<PathBuf> {
        // Hashes come from the index; anything but 40 hex digits is no object.
        (hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| self.objects.join(&hash[..2]).join(hash))
    }
}

impl Source for AssetIndex {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(file) = self
            .by_path
            .get(path)
            .and_then(|hash| self.object_path(hash))
        else {
            return Ok(None);
        };
        fs::read(file).map(Some)
    }

    fn files(&self) -> io::Result<Vec<String>> {
        Ok(self.by_path.keys().cloned().collect())
    }
}

/// Sources read one after another, the first that has a file winning: the vanilla pack, the jar
/// before the asset index, as Java pushes them.
pub struct Layered(pub Vec<Box<dyn Source>>);

impl Source for Layered {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        for source in &self.0 {
            if let Some(bytes) = source.read(path)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    fn files(&self) -> io::Result<Vec<String>> {
        let mut files = Vec::new();
        for source in &self.0 {
            files.extend(source.files()?);
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use super::*;

    /// A zip at `path` holding `files`.
    pub fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn zips_and_folders_read_alike() {
        let dir = tempfile::tempdir().unwrap();
        zip_with(
            &dir.path().join("p.zip"),
            &[("pack.mcmeta", b"{}"), ("assets/a/x.png", b"x")],
        );
        fs::create_dir_all(dir.path().join("p/assets/a")).unwrap();
        fs::write(dir.path().join("p/pack.mcmeta"), b"{}").unwrap();
        fs::write(dir.path().join("p/assets/a/x.png"), b"x").unwrap();

        let zip = Zip::open(&dir.path().join("p.zip")).unwrap();
        let folder = Folder::new(dir.path().join("p"));
        for source in [&zip as &dyn Source, &folder] {
            assert_eq!(
                source.read("assets/a/x.png").unwrap().as_deref(),
                Some(&b"x"[..])
            );
            assert_eq!(source.read("assets/a/missing.png").unwrap(), None);
            assert_eq!(source.read("assets/a").unwrap(), None);
            assert_eq!(source.files().unwrap(), ["assets/a/x.png", "pack.mcmeta"]);
        }
        assert_eq!(folder.read("../p.zip").unwrap(), None);
    }

    #[test]
    fn the_asset_index_reads_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        let hash = "a9993e364706816aba3e25717850c26c9cd0d89d";
        fs::create_dir_all(dir.path().join("indexes")).unwrap();
        fs::create_dir_all(dir.path().join("objects/a9")).unwrap();
        fs::write(
            dir.path().join("indexes/29.json"),
            format!(
                r#"{{"objects": {{"minecraft/sounds/x.ogg": {{"hash": "{hash}", "size": 3}}}}}}"#
            ),
        )
        .unwrap();
        fs::write(dir.path().join("objects/a9").join(hash), b"abc").unwrap();
        let index = AssetIndex::open(dir.path(), "29").unwrap();
        assert_eq!(
            index
                .read("assets/minecraft/sounds/x.ogg")
                .unwrap()
                .as_deref(),
            Some(&b"abc"[..])
        );
        assert_eq!(index.files().unwrap(), ["assets/minecraft/sounds/x.ogg"]);
    }
}
