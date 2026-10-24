//! The proof the mod writes (DESIGN.md §3.1), and checking the game's files against it (§3.2).

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha1::{Digest, Sha1};

/// The proof format this build reads; the mod's `FORMAT`.
const FORMAT: u32 = 1;

#[derive(Debug, Deserialize)]
pub struct Proof {
    pub format: u32,
    pub release: String,
    pub client_jar: GameFile,
    pub assets: Assets,
}

#[derive(Debug, Deserialize)]
pub struct GameFile {
    pub path: PathBuf,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
pub struct Assets {
    /// The launcher's asset store: `indexes/<index>.json` and `objects/<hash prefix>/<hash>`.
    pub directory: PathBuf,
    pub index: String,
    pub index_sha1: String,
}

impl Proof {
    pub fn read(path: &Path) -> Result<Self> {
        let text =
            fs::read(path).with_context(|| format!("reading the proof {}", path.display()))?;
        let proof: Self = serde_json::from_slice(&text)
            .with_context(|| format!("reading the proof {}", path.display()))?;
        if proof.format != FORMAT {
            bail!(
                "the proof is format {}, this build reads format {FORMAT}; update the mod or this source",
                proof.format
            );
        }
        Ok(proof)
    }

    pub fn index_path(&self) -> PathBuf {
        self.assets
            .directory
            .join("indexes")
            .join(format!("{}.json", self.assets.index))
    }

    /// Checks that the client jar and asset index are still the files the mod saw, so a launcher
    /// update or a modified jar isn't converted by mistake.
    pub fn verify(&self) -> Result<()> {
        expect_sha1(&self.client_jar.path, &self.client_jar.sha1)?;
        expect_sha1(&self.index_path(), &self.assets.index_sha1)
    }
}

fn expect_sha1(file: &Path, expected: &str) -> Result<()> {
    let actual = sha1_of(file).with_context(|| format!("reading {}", file.display()))?;
    if actual != expected {
        bail!(
            "{} has changed since the proof was written (SHA-1 {actual}, expected {expected}); run \
             the mod's check again",
            file.display()
        );
    }
    Ok(())
}

pub fn sha1_of(file: &Path) -> io::Result<String> {
    let mut hasher = Sha1::new();
    let mut reader = File::open(file)?;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(&buffer[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof_with(dir: &Path, jar_sha1: &str) -> Proof {
        Proof {
            format: FORMAT,
            release: "1.21.11".into(),
            client_jar: GameFile {
                path: dir.join("client.jar"),
                sha1: jar_sha1.into(),
            },
            assets: Assets {
                directory: dir.to_owned(),
                index: "29".into(),
                // SHA-1 of "{}".
                index_sha1: "bf21a9e8fbc5a3846fb05b4fa0859e0917b2202f".into(),
            },
        }
    }

    fn game_files() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("client.jar"), b"abc").unwrap();
        fs::create_dir(dir.path().join("indexes")).unwrap();
        fs::write(dir.path().join("indexes/29.json"), b"{}").unwrap();
        dir
    }

    #[test]
    fn matching_files_verify() {
        let dir = game_files();
        // FIPS 180's SHA-1 of "abc".
        proof_with(dir.path(), "a9993e364706816aba3e25717850c26c9cd0d89d")
            .verify()
            .unwrap();
    }

    #[test]
    fn a_changed_jar_is_refused() {
        let dir = game_files();
        let error = proof_with(dir.path(), "0000000000000000000000000000000000000000")
            .verify()
            .unwrap_err();
        assert!(error.to_string().contains("has changed"), "{error}");
    }

    #[test]
    fn reads_what_the_mod_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("1.21.11.json");
        // Shaped like the mod's output (account and time are ignored here).
        fs::write(
            &path,
            r#"{"format": 1, "release": "1.21.11",
            "account": {"name": "x", "uuid": "y"}, "checked_at": "z",
            "client_jar": {"path": "/a/client.jar", "sha1": "ab"},
            "assets": {"directory": "/a/assets", "index": "29", "index_sha1": "cd"}}"#,
        )
        .unwrap();
        let proof = Proof::read(&path).unwrap();
        assert_eq!(proof.release, "1.21.11");
        assert_eq!(proof.index_path(), Path::new("/a/assets/indexes/29.json"));
    }
}
