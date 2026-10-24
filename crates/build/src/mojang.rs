//! Mojang's public download metadata: the version manifest, a version's own file, and its asset
//! index. Only the fields the build uses are read; the rest are ignored.

use std::collections::BTreeMap;

use serde::Deserialize;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
/// Where asset objects live, by hash: `<RESOURCES_URL>/<first two hex digits>/<hash>`.
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

/// Every version Mojang has published, newest first.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// The version's own file, and its SHA-1.
    pub url: String,
    pub sha1: String,
    /// ISO 8601, e.g. `2018-07-18T15:11:46+00:00`, which compares correctly as text.
    pub release_time: String,
}

/// A version's own file (`<id>.json`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub downloads: Downloads,
    pub asset_index: AssetIndexRef,
}

#[derive(Debug, Deserialize)]
pub struct Downloads {
    pub client: Download,
}

#[derive(Debug, Deserialize)]
pub struct Download {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// The asset index: every object outside the jar (sounds, most language files), by path.
#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

impl AssetObject {
    pub fn url(&self) -> String {
        format!("{RESOURCES_URL}/{}/{}", &self.hash[..2], self.hash)
    }
}

impl Manifest {
    pub fn find(&self, id: &str) -> Option<&ManifestEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields_it_needs() {
        // Shaped like Mojang's, with made-up values; unknown fields are ignored.
        let manifest: Manifest = serde_json::from_str(
            r#"{"latest": {"release": "9.9"}, "versions": [{"id": "9.9", "type": "release",
            "url": "https://example.invalid/9.9.json", "time": "x",
            "releaseTime": "2030-01-01T00:00:00+00:00", "sha1": "ab", "complianceLevel": 1}]}"#,
        )
        .unwrap();
        let entry = manifest.find("9.9").unwrap();
        assert_eq!(entry.kind, "release");
        assert_eq!(entry.release_time, "2030-01-01T00:00:00+00:00");
        assert!(manifest.find("1.0").is_none());
    }

    #[test]
    fn object_urls_are_by_hash() {
        let object = AssetObject {
            hash: "bdf48ef6b5d0d23bbb02e17d04865216179f510a".into(),
            size: 0,
        };
        assert_eq!(
            object.url(),
            "https://resources.download.minecraft.net/bd/bdf48ef6b5d0d23bbb02e17d04865216179f510a"
        );
    }
}
