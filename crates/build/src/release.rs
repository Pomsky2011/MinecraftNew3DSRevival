//! Which releases can be build bases (DESIGN.md §1.1.1, §5.6).

use anyhow::{Result, bail};

use crate::mojang::{Manifest, ManifestEntry};

/// Releases this source implements, one per `mc*` feature of `n3dsr-core` (a test keeps the two
/// in step). Newest first.
pub const SUPPORTED: &[&str] = &["1.21.11"];

/// The first release after the Flattening; nothing older can be a build base.
pub const FLOOR: &str = "1.13";

/// The manifest's entry for `id`, if it can be a build base.
pub fn choose<'m>(manifest: &'m Manifest, id: &str) -> Result<&'m ManifestEntry> {
    let Some(entry) = manifest.find(id) else {
        bail!("Mojang lists no version {id:?}");
    };
    if entry.kind != "release" {
        bail!(
            "{id} is a {}, not a release; build bases are releases",
            entry.kind
        );
    }
    let Some(floor) = manifest.find(FLOOR) else {
        bail!("Mojang's manifest no longer lists {FLOOR}");
    };
    if entry.release_time < floor.release_time {
        bail!(
            "{id} is older than {FLOOR}: 1.12.2 and earlier aren't supported, since the Flattening \
             reorganised blocks, items, saves and data"
        );
    }
    if !SUPPORTED.contains(&id) {
        bail!(
            "{id} isn't implemented yet; this source supports {}",
            SUPPORTED.join(", ")
        );
    }
    Ok(entry)
}

/// `1.21.11` → `mc1_21_11`.
pub fn feature(id: &str) -> String {
    format!("mc{}", id.replace('.', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, kind: &str, release_time: &str) -> ManifestEntry {
        ManifestEntry {
            id: id.into(),
            kind: kind.into(),
            url: String::new(),
            sha1: String::new(),
            release_time: release_time.into(),
        }
    }

    fn manifest() -> Manifest {
        Manifest {
            versions: vec![
                entry("26.4-snapshot-5", "snapshot", "2026-10-20T00:00:00+00:00"),
                entry("26.3", "release", "2026-09-01T00:00:00+00:00"),
                entry("1.21.11", "release", "2025-12-09T12:23:30+00:00"),
                entry("1.13", "release", "2018-07-18T15:11:46+00:00"),
                entry("1.12.2", "release", "2017-09-18T08:39:46+00:00"),
            ],
        }
    }

    #[test]
    fn a_supported_release_is_chosen() {
        assert_eq!(choose(&manifest(), "1.21.11").unwrap().id, "1.21.11");
    }

    #[test]
    fn what_cant_be_a_build_base_is_refused() {
        let refusal = |id| choose(&manifest(), id).unwrap_err().to_string();
        assert!(refusal("1.12.2").contains("Flattening"));
        assert!(refusal("26.4-snapshot-5").contains("not a release"));
        assert!(refusal("26.3").contains("isn't implemented yet"));
        assert!(refusal("2.0").contains("no version"));
    }

    #[test]
    fn supported_matches_the_core_features() {
        let manifest = include_str!("../../core/Cargo.toml");
        let mut features: Vec<&str> = manifest
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(name, _)| name))
            .filter(|name| name.starts_with("mc"))
            .collect();
        features.sort();
        let mut supported: Vec<String> = SUPPORTED.iter().map(|id| feature(id)).collect();
        supported.sort();
        assert_eq!(features, supported);
    }
}
