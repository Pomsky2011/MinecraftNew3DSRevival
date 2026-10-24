//! A resource pack: its files, and what its `pack.mcmeta` says (DESIGN.md §3.5). The format rules
//! are 1.21.11's (`PackFormat`, `PackMetadataSection`, `OverlayMetadataSection`,
//! `ResourceFilterSection`), reimplemented.

use std::io;

use regex::Regex;
use serde_json::Value;

use crate::source::Source;

/// A resource pack format, `major.minor` (1.21.9 onward; earlier ones are `major.0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Format {
    pub major: u32,
    pub minor: u32,
}

impl Format {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

/// The build base's own resource pack format.
#[cfg(feature = "mc1_21_11")]
pub const CURRENT: Format = Format::new(75, 0);

/// The last format before `min_format`/`max_format`: packs reaching back to it or below must also
/// say `supported_formats` (and `pack_format`), for older games to read them.
#[cfg(feature = "mc1_21_11")]
const LAST_PRE_MINOR: u32 = 64;

/// An inclusive range of formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formats {
    pub min: Format,
    pub max: Format,
}

impl Formats {
    pub fn contains(&self, format: Format) -> bool {
        self.min <= format && format <= self.max
    }
}

/// Whether a pack suits the build base: Java loads an incompatible pack only once the player has
/// confirmed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compatibility {
    TooOld,
    TooNew,
    Compatible,
}

impl Formats {
    pub fn compatibility(&self, current: Format) -> Compatibility {
        if self.max < current {
            Compatibility::TooOld
        } else if current < self.min {
            Compatibility::TooNew
        } else {
            Compatibility::Compatible
        }
    }
}

/// One `filter.block` entry: regexes searched for (Java's `Pattern.asPredicate`), anywhere in the
/// namespace and the path; a missing one matches everything.
#[derive(Debug)]
pub struct BlockPattern {
    namespace: Option<Regex>,
    path: Option<Regex>,
}

impl BlockPattern {
    pub fn matches(&self, namespace: &str, path: &str) -> bool {
        self.namespace
            .as_ref()
            .is_none_or(|r| r.is_match(namespace))
            && self.path.as_ref().is_none_or(|r| r.is_match(path))
    }
}

/// A pack: its files and what its `pack.mcmeta` says.
pub struct Pack {
    pub id: String,
    pub formats: Formats,
    /// Overlay folders that apply to the build base, highest priority first.
    overlays: Vec<String>,
    pub filter: Vec<BlockPattern>,
    source: Box<dyn Source>,
}

impl Pack {
    /// A pack read from its `pack.mcmeta`; `Err` with Java's reason if Java wouldn't load it.
    pub fn open(id: String, source: Box<dyn Source>) -> Result<Self, String> {
        let meta = source
            .read("pack.mcmeta")
            .map_err(|e| format!("couldn't read pack.mcmeta: {e}"))?
            .ok_or("no pack.mcmeta")?;
        let meta: Value =
            serde_json::from_slice(&meta).map_err(|e| format!("pack.mcmeta isn't JSON: {e}"))?;
        let pack = meta
            .get("pack")
            .ok_or("pack.mcmeta has no \"pack\" section")?;
        if pack.get("description").is_none() {
            return Err("pack.mcmeta's \"pack\" has no description".into());
        }
        let formats = Declared::read(pack, "supported_formats")?.validate(
            true,
            false,
            "Pack",
            "supported_formats",
        )?;
        let overlays = match meta.get("overlays") {
            Some(section) => overlays(section)?,
            None => Vec::new(),
        };
        let filter = match meta.get("filter") {
            Some(section) => filter(section)?,
            None => Vec::new(),
        };
        Ok(Self {
            id,
            formats,
            overlays,
            filter,
            source,
        })
    }

    /// A pack whose formats are given rather than read: the vanilla pack, which is the build
    /// base's own format.
    pub fn built_in(id: String, source: Box<dyn Source>) -> Self {
        Self {
            id,
            formats: Formats {
                min: CURRENT,
                max: Format::new(CURRENT.major, u32::MAX),
            },
            overlays: Vec::new(),
            filter: Vec::new(),
            source,
        }
    }

    /// The pack's file at `path` (`assets/<namespace>/...`), its overlays before its base.
    pub fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        for overlay in &self.overlays {
            if let Some(bytes) = self.source.read(&format!("{overlay}/{path}"))? {
                return Ok(Some(bytes));
            }
        }
        self.source.read(path)
    }

    /// Every path the pack has, overlays merged in.
    pub fn files(&self) -> io::Result<Vec<String>> {
        let mut files = Vec::new();
        for file in self.source.files()? {
            let in_overlay = self.overlays.iter().find_map(|overlay| {
                file.strip_prefix(overlay.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
            });
            files.push(in_overlay.map_or_else(|| file.clone(), str::to_owned));
        }
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// Whether this pack's filter hides `namespace:path` in the packs below it.
    pub fn filters(&self, namespace: &str, path: &str) -> bool {
        self.filter
            .iter()
            .any(|pattern| pattern.matches(namespace, path))
    }
}

impl std::fmt::Debug for Pack {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Pack")
            .field("id", &self.id)
            .field("formats", &self.formats)
            .field("overlays", &self.overlays)
            .finish_non_exhaustive()
    }
}

/// What a `pack` or overlay entry declares, before Java's checks.
#[derive(Debug, Default)]
struct Declared {
    min: Option<Format>,
    max: Option<Format>,
    format: Option<u32>,
    supported: Option<(u32, u32)>,
}

impl Declared {
    fn read(section: &Value, supported_key: &str) -> Result<Self, String> {
        Ok(Self {
            min: section
                .get("min_format")
                .map(|v| format_of(v, 0))
                .transpose()?,
            max: section
                .get("max_format")
                .map(|v| format_of(v, u32::MAX))
                .transpose()?,
            format: section
                .get("pack_format")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or("pack_format isn't a number")
                })
                .transpose()?,
            supported: section.get(supported_key).map(int_range).transpose()?,
        })
    }

    /// Java's `IntermediaryFormat.validate`: the formats the entry supports, or why it's refused.
    /// `pack` is true for a pack's own declaration, false for an overlay's; `strict` is whether
    /// every overlay must say its range in the old way too.
    fn validate(
        &self,
        pack: bool,
        strict: bool,
        what: &str,
        supported_key: &str,
    ) -> Result<Formats, String> {
        let current = LAST_PRE_MINOR;
        if self.min.is_some() != self.max.is_some() {
            return Err(format!(
                "{what} missing field, must declare both min_format and max_format"
            ));
        }
        if strict && self.supported.is_none() {
            return Err(format!(
                "{what} missing required field {supported_key}, must be present in all overlays for any overlays to work across game versions"
            ));
        }
        if let (Some(min), Some(max)) = (self.min, self.max) {
            let (j, k) = (min.major, max.major);
            if min > max {
                return Err(format!(
                    "{what} min_format ({min:?}) is greater than max_format ({max:?})"
                ));
            }
            if j > current && !strict {
                if self.supported.is_some() {
                    return Err(format!(
                        "{what} key {supported_key} is deprecated starting from pack format {}. Remove {supported_key} from your pack.mcmeta.",
                        current + 1
                    ));
                }
                if pack && self.format.is_some() {
                    self.check_format_in(j, k)?;
                }
            } else {
                let Some((low, high)) = self.supported else {
                    return Err(format!(
                        "{what} declares support for format {j}, but game versions supporting formats 17 to {current} require a {supported_key} field"
                    ));
                };
                if low != j {
                    return Err(format!(
                        "{what} version declaration mismatch between {supported_key} and min_format"
                    ));
                }
                if high != k && high != current {
                    return Err(format!(
                        "{what} version declaration mismatch between {supported_key} and max_format"
                    ));
                }
                if pack {
                    if self.format.is_none() {
                        return Err(format!(
                            "{what} declares support for formats up to {current}, but game versions supporting formats 17 to {current} require a pack_format field"
                        ));
                    }
                    self.check_format_in(j, k)?;
                }
            }
            return Ok(Formats { min, max });
        }
        if let Some((j, k)) = self.supported {
            if k > current {
                return Err(format!(
                    "{what} declares support for version newer than {current}, but is missing mandatory fields min_format and max_format"
                ));
            }
            if pack {
                if self.format.is_none() {
                    return Err(format!(
                        "{what} declares support for formats up to {current}, but game versions supporting formats 17 to {current} require a pack_format field"
                    ));
                }
                self.check_format_in(j, k)?;
            }
            return Ok(Formats {
                min: Format::new(j, 0),
                max: Format::new(k, 0),
            });
        }
        if let (true, Some(j)) = (pack, self.format) {
            if j > current {
                return Err(format!(
                    "{what} declares support for version newer than {current}, but is missing mandatory fields min_format and max_format"
                ));
            }
            return Ok(Formats {
                min: Format::new(j, 0),
                max: Format::new(j, 0),
            });
        }
        Err(format!(
            "{what} could not be parsed, missing format version information"
        ))
    }

    /// Java's `validatePackFormatForRange`.
    fn check_format_in(&self, low: u32, high: u32) -> Result<(), String> {
        let format = self.format.expect("checked by the caller");
        if format < low || format > high {
            return Err(format!(
                "Pack declared support for versions {low} to {high} but declared main format is {format}"
            ));
        }
        if format < 15 {
            return Err("Multi-version packs cannot support minimum version of less than 15, since this will leave versions in range unable to load pack.".into());
        }
        Ok(())
    }
}

/// The overlays that apply to the build base, highest priority first (Java reads them last
/// listed first).
fn overlays(section: &Value) -> Result<Vec<String>, String> {
    let entries = section
        .get("entries")
        .and_then(Value::as_array)
        .ok_or("overlays has no entries list")?;
    let mut declared = Vec::new();
    for entry in entries {
        let directory = entry
            .get("directory")
            .and_then(Value::as_str)
            .ok_or("an overlay has no directory")?;
        if directory.is_empty()
            || !directory
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(format!("{directory} is not accepted directory name"));
        }
        let mut formats = Declared::read(entry, "formats")?;
        // An overlay's `format` is its min_format's major, for the checks.
        formats.format = formats.min.map(|min| min.major);
        declared.push((directory.to_owned(), formats));
    }
    // Every overlay must also say `formats` when any of them reaches back to the old formats.
    let lowest = declared
        .iter()
        .map(|(_, d)| d.lowest_major())
        .min()
        .unwrap_or(u32::MAX);
    let strict = lowest <= LAST_PRE_MINOR;
    let mut applicable = Vec::new();
    for (directory, formats) in declared {
        if formats.min.is_none() && formats.max.is_none() && formats.supported.is_none() {
            // Java warns and skips an entry with no formats at all.
            continue;
        }
        let range = formats.validate(
            false,
            strict,
            &format!("Overlay \"{directory}\""),
            "formats",
        )?;
        if range.contains(CURRENT) {
            applicable.push(directory);
        }
    }
    applicable.reverse();
    Ok(applicable)
}

impl Declared {
    /// Java's `effectiveMinMajorVersion`.
    fn lowest_major(&self) -> u32 {
        match (self.min, self.supported) {
            (Some(min), Some((low, _))) => min.major.min(low),
            (Some(min), None) => min.major,
            (None, Some((low, _))) => low,
            (None, None) => u32::MAX,
        }
    }
}

fn filter(section: &Value) -> Result<Vec<BlockPattern>, String> {
    let block = section
        .get("block")
        .and_then(Value::as_array)
        .ok_or("filter has no block list")?;
    block
        .iter()
        .map(|entry| {
            let regex = |key: &str| {
                entry
                    .get(key)
                    .map(|v| {
                        let pattern = v.as_str().ok_or(format!("filter {key} isn't a string"))?;
                        Regex::new(pattern).map_err(|e| format!("filter {key} {pattern:?}: {e}"))
                    })
                    .transpose()
            };
            Ok(BlockPattern {
                namespace: regex("namespace")?,
                path: regex("path")?,
            })
        })
        .collect()
}

/// A format as JSON: `75` (minor `default_minor`) or `[75, 1]`.
fn format_of(value: &Value, default_minor: u32) -> Result<Format, String> {
    let int = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    match value {
        Value::Array(list) if (1..=256).contains(&list.len()) => {
            let parts: Option<Vec<u32>> = list.iter().map(int).collect();
            let parts = parts.ok_or("a format list must hold numbers")?;
            Ok(Format::new(
                parts[0],
                parts.get(1).copied().unwrap_or(default_minor),
            ))
        }
        v => int(v)
            .map(|major| Format::new(major, default_minor))
            .ok_or_else(|| format!("{v} isn't a format")),
    }
}

/// An inclusive int range as JSON: `5`, `[5, 9]` or `{"min_inclusive": 5, "max_inclusive": 9}`.
fn int_range(value: &Value) -> Result<(u32, u32), String> {
    let int = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    let (low, high) = match value {
        Value::Array(list) if list.len() == 2 => (int(&list[0]), int(&list[1])),
        Value::Object(map) => (
            map.get("min_inclusive").and_then(int),
            map.get("max_inclusive").and_then(int),
        ),
        v => (int(v), int(v)),
    };
    match (low, high) {
        (Some(low), Some(high)) if low <= high => Ok((low, high)),
        (Some(_), Some(_)) => {
            Err("min_inclusive must be less than or equal to max_inclusive".into())
        }
        _ => Err(format!("{value} isn't a range")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Folder;

    /// A pack of just `pack.mcmeta`; `open` reads it at once, so the folder can go after.
    fn pack(meta: &str) -> Result<Pack, String> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pack.mcmeta"), meta).unwrap();
        Pack::open("test".into(), Box::new(Folder::new(dir.path().to_owned())))
    }

    #[test]
    fn an_old_pack_format_is_too_old_but_loads() {
        let p = pack(r#"{"pack": {"description": "", "pack_format": 46}}"#).unwrap();
        assert_eq!(p.formats.compatibility(CURRENT), Compatibility::TooOld);
    }

    #[test]
    fn new_style_ranges_reach_the_current_format() {
        let p =
            pack(r#"{"pack": {"description": "", "min_format": 75, "max_format": 80}}"#).unwrap();
        assert_eq!(p.formats.min, Format::new(75, 0));
        assert_eq!(p.formats.max, Format::new(80, u32::MAX));
        assert_eq!(p.formats.compatibility(CURRENT), Compatibility::Compatible);
    }

    #[test]
    fn old_and_new_declarations_must_agree() {
        // Reaching back to the old formats needs supported_formats and pack_format too.
        let missing = pack(r#"{"pack": {"description": "", "min_format": 34, "max_format": 75}}"#);
        assert!(
            missing
                .unwrap_err()
                .contains("require a supported_formats field")
        );
        let ok = pack(
            r#"{"pack": {"description": "", "min_format": 34, "max_format": 75,
            "supported_formats": [34, 64], "pack_format": 34}}"#,
        );
        assert_eq!(
            ok.unwrap().formats.compatibility(CURRENT),
            Compatibility::Compatible
        );
        let only_new_too_new = pack(r#"{"pack": {"description": "", "pack_format": 70}}"#);
        assert!(
            only_new_too_new
                .unwrap_err()
                .contains("missing mandatory fields")
        );
    }

    #[test]
    fn overlays_apply_by_format_last_listed_first() {
        let p = pack(
            r#"{"pack": {"description": "", "min_format": 70, "max_format": 80},
            "overlays": {"entries": [
                {"directory": "old", "min_format": 70, "max_format": 74},
                {"directory": "a", "min_format": 75, "max_format": 75},
                {"directory": "b", "min_format": [75, 0], "max_format": 99}]}}"#,
        )
        .unwrap();
        assert_eq!(p.overlays, ["b", "a"]);
    }

    #[test]
    fn filters_search_like_java() {
        let p = pack(
            r#"{"pack": {"description": "", "pack_format": 46},
            "filter": {"block": [{"namespace": "minecraft", "path": "textures/block/.*_ore"}]}}"#,
        )
        .unwrap();
        // asPredicate finds the pattern anywhere: "deepslate_iron_ore.png" matches too.
        assert!(p.filters("minecraft", "textures/block/deepslate_iron_ore.png"));
        assert!(!p.filters("minecraft", "textures/block/stone.png"));
        assert!(!p.filters("other", "textures/block/iron_ore.png"));
    }
}
