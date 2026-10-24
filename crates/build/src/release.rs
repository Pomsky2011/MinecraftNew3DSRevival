//! Which releases this source can build (DESIGN.md §1.1.1, §5.6).

use anyhow::{Result, bail};

/// Releases this source implements, one per `mc*` feature of `n3dsr-core` (a test keeps the two
/// in step). Newest first.
pub const SUPPORTED: &[&str] = &["1.21.11"];

pub fn check(release: &str) -> Result<()> {
    if !SUPPORTED.contains(&release) {
        bail!(
            "{release} isn't implemented yet; this source builds {}",
            SUPPORTED.join(", ")
        );
    }
    Ok(())
}

/// `1.21.11` → `mc1_21_11`.
#[cfg(test)]
fn feature(release: &str) -> String {
    format!("mc{}", release.replace('.', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_releases_are_refused() {
        check("1.21.11").unwrap();
        assert!(
            check("1.16.5")
                .unwrap_err()
                .to_string()
                .contains("isn't implemented")
        );
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
