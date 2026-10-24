//! Downloads, checked against Mojang's SHA-1s and kept in the user's cache directory
//! (DESIGN.md §3.2). Nothing lands in the repository.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha1::{Digest, Sha1};

pub struct Cache {
    root: PathBuf,
}

/// A file Mojang lists, with the size and SHA-1 it should have.
pub struct Expected<'a> {
    pub url: &'a str,
    pub sha1: &'a str,
    pub size: Option<u64>,
}

impl Cache {
    /// `~/.cache/n3dsr` on Linux, or the platform's equivalent.
    pub fn new() -> Result<Self> {
        let base = dirs::cache_dir().context("no cache directory on this system")?;
        Ok(Self::at(base.join("n3dsr")))
    }

    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The file at `path` (relative to the cache), downloaded unless a copy with the right
    /// SHA-1 is already there.
    pub fn get(&self, path: &str, expected: &Expected) -> Result<PathBuf> {
        let file = self.root.join(path);
        if file.exists() && sha1_of(&file)? == expected.sha1 {
            return Ok(file);
        }
        fs::create_dir_all(file.parent().unwrap())?;
        download(expected, &file).with_context(|| format!("downloading {}", expected.url))?;
        Ok(file)
    }

    /// A file with no SHA-1 to check (the manifest itself), always fetched fresh, the cached copy
    /// standing in when the network fails.
    pub fn get_fresh(&self, path: &str, url: &str) -> Result<PathBuf> {
        let file = self.root.join(path);
        fs::create_dir_all(file.parent().unwrap())?;
        let fetched = (|| -> Result<()> {
            let partial = partial_path(&file);
            let mut response = ureq::get(url).call()?;
            io::copy(
                &mut response.body_mut().as_reader(),
                &mut File::create(&partial)?,
            )?;
            fs::rename(&partial, &file)?;
            Ok(())
        })();
        match fetched {
            Ok(()) => Ok(file),
            Err(e) if file.exists() => {
                eprintln!("warning: couldn't fetch {url} ({e:#}); using the cached copy");
                Ok(file)
            }
            Err(e) => Err(e.context(format!("fetching {url}"))),
        }
    }
}

/// Downloads to a partial file, hashing as it goes, and moves it into place only if the size and
/// SHA-1 match; a failed or tampered download never sits where a good one is expected.
fn download(expected: &Expected, file: &Path) -> Result<()> {
    let partial = partial_path(file);
    let mut response = ureq::get(expected.url).call()?;
    let mut body = response.body_mut().with_config();
    if let Some(size) = expected.size {
        // A server sending more than Mojang listed fails here rather than filling the disk.
        body = body.limit(size);
    }
    let mut out = File::create(&partial)?;
    let (sha1, written) = hash_copy(&mut body.reader(), &mut out)?;
    out.sync_all()?;
    drop(out);

    if expected.size.is_some_and(|size| size != written) || sha1 != expected.sha1 {
        fs::remove_file(&partial)?;
        bail!(
            "got {written} bytes with SHA-1 {sha1}, expected {} with {}",
            expected.size.map_or("any size".into(), |s| s.to_string()),
            expected.sha1
        );
    }
    fs::rename(&partial, file)?;
    Ok(())
}

fn partial_path(file: &Path) -> PathBuf {
    let mut name = file.file_name().unwrap().to_owned();
    name.push(".partial");
    file.with_file_name(name)
}

pub fn sha1_of(file: &Path) -> Result<String> {
    Ok(hash_copy(&mut File::open(file)?, &mut io::sink())?.0)
}

/// Copies `from` into `to`, returning the SHA-1 of what was copied (hex) and its length.
fn hash_copy(from: &mut impl Read, to: &mut impl Write) -> io::Result<(String, u64)> {
    let mut hasher = Sha1::new();
    let mut length = 0u64;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let n = from.read(&mut buffer)?;
        if n == 0 {
            return Ok((hex::encode(hasher.finalize()), length));
        }
        hasher.update(&buffer[..n]);
        to.write_all(&buffer[..n])?;
        length += n as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_the_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("abc");
        fs::write(&file, b"abc").unwrap();
        // FIPS 180's test vector for "abc".
        assert_eq!(
            sha1_of(&file).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn a_cached_file_with_the_right_sha1_is_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().to_owned());
        fs::write(dir.path().join("abc"), b"abc").unwrap();
        // The URL can't be fetched, so this only passes if the cached copy is used.
        let expected = Expected {
            url: "http://invalid.invalid/abc",
            sha1: "a9993e364706816aba3e25717850c26c9cd0d89d",
            size: Some(3),
        };
        assert_eq!(cache.get("abc", &expected).unwrap(), dir.path().join("abc"));
    }
}
