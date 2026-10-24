//! Block and item model files, and blockstate files, as 1.21.11 reads them (`BlockModel`,
//! `BlockElement`, `BlockElementFace`, `BlockElementRotation`, `BlockModelDefinition`, `Variant`,
//! multipart `Condition`s; reimplemented, DESIGN.md §3.7). Parsing only: `bake` turns them into
//! quads.

use serde_json::Value;

/// `namespace:path`.
pub type Id = String;

/// Java's `Direction`, in its declaration order, which is the order faces are baked in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub const ALL: [Direction; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];

    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "down" => Self::Down,
            "up" => Self::Up,
            "north" => Self::North,
            "south" => Self::South,
            "west" => Self::West,
            "east" => Self::East,
            _ => return None,
        })
    }

    pub fn axis(self) -> Axis {
        match self {
            Self::Down | Self::Up => Axis::Y,
            Self::North | Self::South => Axis::Z,
            Self::West | Self::East => Axis::X,
        }
    }

    /// The unit vector it points along.
    pub fn normal(self) -> [i32; 3] {
        match self {
            Self::Down => [0, -1, 0],
            Self::Up => [0, 1, 0],
            Self::North => [0, 0, -1],
            Self::South => [0, 0, 1],
            Self::West => [-1, 0, 0],
            Self::East => [1, 0, 0],
        }
    }

    pub fn from_normal(normal: [i32; 3]) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.normal() == normal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// A quarter turn: 0, 90, 180 or 270 degrees, as `Quadrant`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct Quadrant(pub u8);

impl Quadrant {
    /// Any multiple of 90 (negative ones too), as Java's codec.
    fn parse(degrees: i64) -> Result<Self, String> {
        match degrees.rem_euclid(360) {
            d @ (0 | 90 | 180 | 270) => Ok(Self((d / 90) as u8)),
            _ => Err(format!(
                "Invalid rotation {degrees} found, only 0/90/180/270 allowed"
            )),
        }
    }
}

/// A model file.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub parent: Option<Id>,
    /// `None` when the file has no `elements`, so the parent's apply.
    pub elements: Option<Vec<Element>>,
    pub textures: Vec<(String, Slot)>,
    pub ambient_occlusion: Option<bool>,
    /// `gui_light`, `display`: kept for items.
    pub gui_light: Option<String>,
    pub display: Option<Value>,
}

/// A texture variable's value: a texture, or another variable (`#name`).
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    Texture(Id),
    Reference(String),
}

#[derive(Clone, Debug)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub rotation: Option<ElementRotation>,
    pub shade: bool,
    pub light_emission: u8,
    /// In `Direction` order.
    pub faces: Vec<(Direction, Face)>,
}

#[derive(Clone, Debug)]
pub struct ElementRotation {
    /// In blocks (the file's sixteenths × 0.0625).
    pub origin: [f32; 3],
    pub value: RotationValue,
    pub rescale: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum RotationValue {
    /// Degrees about one axis.
    Axis(Axis, f32),
    /// Degrees about x, y and z, applied z, y, x as JOML's `rotationZYX`.
    Euler([f32; 3]),
}

#[derive(Clone, Debug)]
pub struct Face {
    pub cull: Option<Direction>,
    pub tint: i32,
    pub texture: String,
    /// min u, min v, max u, max v, in sixteenths; `None` for the element's default.
    pub uv: Option<[f32; 4]>,
    pub rotation: Quadrant,
}

impl Model {
    pub fn parse(json: &[u8]) -> Result<Self, String> {
        let json: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let object = json.as_object().ok_or("a model isn't an object")?;
        let elements = match object.get("elements") {
            None => None,
            Some(list) => Some(
                list.as_array()
                    .ok_or("elements isn't a list")?
                    .iter()
                    .map(Element::parse)
                    .collect::<Result<_, _>>()?,
            ),
        };
        let mut textures = Vec::new();
        if let Some(map) = object.get("textures") {
            for (key, value) in map.as_object().ok_or("textures isn't an object")? {
                let value = value
                    .as_str()
                    .ok_or_else(|| format!("texture {key} isn't a string"))?;
                let slot = match value.strip_prefix('#') {
                    Some(reference) => Slot::Reference(reference.to_owned()),
                    None => Slot::Texture(id(value)?),
                };
                textures.push((key.clone(), slot));
            }
        }
        let parent = match object.get("parent") {
            None => None,
            Some(p) => {
                let p = p.as_str().ok_or("parent isn't a string")?;
                (!p.is_empty()).then(|| id(p)).transpose()?
            }
        };
        Ok(Self {
            parent,
            elements,
            textures,
            ambient_occlusion: object
                .get("ambientocclusion")
                .map(|v| boolean(v, "ambientocclusion"))
                .transpose()?,
            gui_light: object
                .get("gui_light")
                .and_then(Value::as_str)
                .map(str::to_owned),
            display: object.get("display").cloned(),
        })
    }
}

impl Element {
    fn parse(json: &Value) -> Result<Self, String> {
        let from = position(json, "from")?;
        let to = position(json, "to")?;
        let rotation = json
            .get("rotation")
            .map(ElementRotation::parse)
            .transpose()?;
        let faces_json = json
            .get("faces")
            .and_then(Value::as_object)
            .ok_or("an element has no faces")?;
        let mut faces = Vec::new();
        for (name, face) in faces_json {
            let direction =
                Direction::by_name(name).ok_or_else(|| format!("Unknown facing: {name}"))?;
            faces.retain(|(d, _)| *d != direction);
            faces.push((direction, Face::parse(face)?));
        }
        if faces.is_empty() {
            return Err("Expected between 1 and 6 unique faces, got 0".into());
        }
        faces.sort_by_key(|(d, _)| *d);
        let shade = match json.get("shade") {
            None => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err("Expected 'shade' to be a Boolean".into()),
        };
        let light_emission =
            match json.get("light_emission") {
                None => 0,
                Some(v) => match v.as_i64() {
                    Some(n @ 0..=15) => n as u8,
                    _ => return Err(
                        "Expected 'light_emission' to be an Integer between (inclusive) 0 and 15"
                            .into(),
                    ),
                },
            };
        Ok(Self {
            from,
            to,
            rotation,
            shade,
            light_emission,
            faces,
        })
    }
}

impl ElementRotation {
    fn parse(json: &Value) -> Result<Self, String> {
        let origin = vector(json, "origin")?.map(|v| v * 0.0625);
        let has = |key: &str| json.get(key).is_some();
        let value = if !has("axis") && !has("angle") {
            if !has("x") && !has("y") && !has("z") {
                return Err("Missing rotation value, expected either 'axis' and 'angle' or 'x', 'y' and 'z'".into());
            }
            let angle = |key: &str| json.get(key).map_or(Ok(0.0), |v| float(v, key));
            RotationValue::Euler([angle("x")?, angle("y")?, angle("z")?])
        } else {
            let axis = json
                .get("axis")
                .and_then(Value::as_str)
                .ok_or("rotation has no axis")?;
            let axis = match axis.to_lowercase().as_str() {
                "x" => Axis::X,
                "y" => Axis::Y,
                "z" => Axis::Z,
                _ => return Err(format!("Invalid rotation axis: {axis}")),
            };
            RotationValue::Axis(
                axis,
                float(json.get("angle").ok_or("rotation has no angle")?, "angle")?,
            )
        };
        let rescale = json
            .get("rescale")
            .map(|v| boolean(v, "rescale"))
            .transpose()?
            .unwrap_or(false);
        Ok(Self {
            origin,
            value,
            rescale,
        })
    }
}

impl Face {
    fn parse(json: &Value) -> Result<Self, String> {
        let texture = json
            .get("texture")
            .and_then(Value::as_str)
            .ok_or("a face has no texture")?;
        // An unknown cullface is no cullface, as Java's `Direction.byName`.
        let cull = json
            .get("cullface")
            .and_then(Value::as_str)
            .and_then(Direction::by_name);
        let tint = json
            .get("tintindex")
            .map(|v| v.as_i64().ok_or("tintindex isn't an int"))
            .transpose()?
            .unwrap_or(-1) as i32;
        let rotation = json
            .get("rotation")
            .map(|v| v.as_i64().ok_or("face rotation isn't an int"))
            .transpose()?
            .map(Quadrant::parse)
            .transpose()?
            .unwrap_or_default();
        let uv = match json.get("uv") {
            None => None,
            Some(list) => {
                let list = list.as_array().ok_or("uv isn't a list")?;
                if list.len() != 4 {
                    return Err(format!("Expected 4 uv values, found: {}", list.len()));
                }
                Some([
                    float(&list[0], "minU")?,
                    float(&list[1], "minV")?,
                    float(&list[2], "maxU")?,
                    float(&list[3], "maxV")?,
                ])
            }
        };
        Ok(Self {
            cull,
            tint,
            texture: texture.to_owned(),
            uv,
            rotation,
        })
    }
}

/// A blockstate file: variants by key, or multipart parts with conditions.
#[derive(Clone, Debug, Default)]
pub struct BlockState {
    /// Variant key (`facing=north,half=top`, or `""` for every state) and its choices.
    pub variants: Vec<(String, Vec<Weighted>)>,
    pub multipart: Vec<Part>,
}

/// One choice of model and orientation, with its weight among the others.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Variant {
    pub model: Id,
    pub x: Quadrant,
    pub y: Quadrant,
    pub z: Quadrant,
    pub uvlock: bool,
}

#[derive(Clone, Debug)]
pub struct Weighted {
    pub variant: Variant,
    pub weight: u32,
}

#[derive(Clone, Debug)]
pub struct Part {
    /// `None`: always applies.
    pub when: Option<Condition>,
    pub apply: Vec<Weighted>,
}

/// A multipart condition: property tests (each value one of `a|b|c`, `!` negating a value), or an
/// `AND`/`OR` of others.
#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    Properties(Vec<(String, Vec<(String, bool)>)>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

impl BlockState {
    pub fn parse(json: &[u8]) -> Result<Self, String> {
        let json: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
        let mut out = Self::default();
        if let Some(variants) = json.get("variants") {
            let variants = variants.as_object().ok_or("variants isn't an object")?;
            if variants.is_empty() {
                return Err("variants is empty".into());
            }
            for (key, choices) in variants {
                out.variants.push((key.clone(), weighted(choices)?));
            }
        }
        if let Some(parts) = json.get("multipart") {
            let parts = parts
                .as_array()
                .filter(|p| !p.is_empty())
                .ok_or("multipart isn't a non-empty list")?;
            for part in parts {
                out.multipart.push(Part {
                    when: part.get("when").map(Condition::parse).transpose()?,
                    apply: weighted(part.get("apply").ok_or("a part has no apply")?)?,
                });
            }
        }
        if out.variants.is_empty() && out.multipart.is_empty() {
            return Err("Neither 'variants' nor 'multipart' found".into());
        }
        Ok(out)
    }
}

impl Condition {
    fn parse(json: &Value) -> Result<Self, String> {
        let map = json.as_object().ok_or("a condition isn't an object")?;
        // A combiner: exactly one key, AND or OR, holding a list.
        if map.len() == 1 {
            let (key, value) = map.iter().next().unwrap();
            if let (Some(op), Some(list)) =
                (["AND", "OR"].iter().find(|op| *op == key), value.as_array())
            {
                let terms = list
                    .iter()
                    .map(Condition::parse)
                    .collect::<Result<_, _>>()?;
                return Ok(if *op == "AND" {
                    Self::And(terms)
                } else {
                    Self::Or(terms)
                });
            }
        }
        if map.is_empty() {
            return Err("an empty condition".into());
        }
        let mut tests = Vec::new();
        for (property, value) in map {
            // A string, or (an older form) an int or a boolean.
            let text = match value {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) if n.as_i64().is_some() => n.to_string(),
                _ => return Err(format!("condition {property} isn't a string")),
            };
            let mut terms = Vec::new();
            for term in text.split('|') {
                let (value, negated) = term.strip_prefix('!').map_or((term, false), |v| (v, true));
                if value.is_empty() {
                    return Err(format!("Empty term in value '{text}'"));
                }
                terms.push((value.to_owned(), negated));
            }
            tests.push((property.clone(), terms));
        }
        Ok(Self::Properties(tests))
    }
}

/// One variant, or a non-empty list of weighted ones.
fn weighted(json: &Value) -> Result<Vec<Weighted>, String> {
    let one = |v: &Value| -> Result<Weighted, String> {
        let weight = match v.get("weight") {
            None => 1,
            Some(w) => w
                .as_u64()
                .filter(|w| *w > 0)
                .ok_or("weight isn't a positive int")? as u32,
        };
        let quadrant = |key: &str| -> Result<Quadrant, String> {
            v.get(key).map_or(Ok(Quadrant(0)), |q| {
                Quadrant::parse(q.as_i64().ok_or_else(|| format!("{key} isn't an int"))?)
            })
        };
        Ok(Weighted {
            variant: Variant {
                model: id(v
                    .get("model")
                    .and_then(Value::as_str)
                    .ok_or("a variant has no model")?)?,
                x: quadrant("x")?,
                y: quadrant("y")?,
                z: quadrant("z")?,
                uvlock: v
                    .get("uvlock")
                    .map(|b| boolean(b, "uvlock"))
                    .transpose()?
                    .unwrap_or(false),
            },
            weight,
        })
    };
    match json {
        Value::Array(list) if !list.is_empty() => list.iter().map(one).collect(),
        Value::Array(_) => Err("an empty variant list".into()),
        single => Ok(vec![one(single)?]),
    }
}

/// `path` or `namespace:path` → `namespace:path`, checked as `Identifier.parse` checks.
pub fn id(text: &str) -> Result<Id, String> {
    let (namespace, path) = text.split_once(':').unwrap_or(("minecraft", text));
    let namespace = if namespace.is_empty() {
        "minecraft"
    } else {
        namespace
    };
    let ok_ns = namespace
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'));
    let ok_path = path
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'));
    if !ok_ns || !ok_path {
        return Err(format!("{text} is not valid resource location"));
    }
    Ok(format!("{namespace}:{path}"))
}

/// A JSON number as Java's `float`: from its decimal text, rounded once.
pub fn float(value: &Value, what: &str) -> Result<f32, String> {
    match value {
        Value::Number(n) => n
            .to_string()
            .parse::<f32>()
            .map_err(|_| format!("{what} isn't a number")),
        // Gson reads a numeric string as a number too.
        Value::String(s) => s
            .trim()
            .parse::<f32>()
            .map_err(|_| format!("{what} isn't a number")),
        _ => Err(format!("{what} isn't a number")),
    }
}

fn vector(json: &Value, key: &str) -> Result<[f32; 3], String> {
    let list = json
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("no {key}"))?;
    if list.len() != 3 {
        return Err(format!("Expected 3 {key} values, found: {}", list.len()));
    }
    Ok([
        float(&list[0], key)?,
        float(&list[1], key)?,
        float(&list[2], key)?,
    ])
}

fn position(json: &Value, key: &str) -> Result<[f32; 3], String> {
    let v = vector(json, key)?;
    if v.iter().any(|c| *c < -16.0 || *c > 32.0) {
        return Err(format!(
            "'{key}' specifier exceeds the allowed boundaries: {v:?}"
        ));
    }
    Ok(v)
}

fn boolean(value: &Value, what: &str) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("{what} isn't true or false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_s_elements_faces_and_textures() {
        let model = Model::parse(
            br##"{"parent": "block/block", "textures": {"all": "block/stone", "side": "#all"},
            "elements": [{"from": [0, 0, 0], "to": [16, 8, 16],
                "rotation": {"origin": [8, 8, 8], "axis": "y", "angle": 22.5, "rescale": true},
                "faces": {"up": {"texture": "#side", "cullface": "up", "uv": [0, 0, 16, 16], "rotation": -90},
                          "down": {"texture": "#all", "tintindex": 0}}}]}"##,
        )
        .unwrap();
        assert_eq!(model.parent.as_deref(), Some("minecraft:block/block"));
        assert!(
            model
                .textures
                .contains(&("side".into(), Slot::Reference("all".into())))
        );
        let element = &model.elements.unwrap()[0];
        // Faces in Direction order, whatever the file's order.
        assert_eq!(
            element.faces.iter().map(|(d, _)| *d).collect::<Vec<_>>(),
            [Direction::Down, Direction::Up]
        );
        assert_eq!(element.faces[1].1.rotation, Quadrant(3));
        let rotation = element.rotation.as_ref().unwrap();
        assert_eq!(rotation.origin, [0.5, 0.5, 0.5]);
        assert!(matches!(rotation.value, RotationValue::Axis(Axis::Y, a) if a == 22.5));
    }

    #[test]
    fn floats_round_once_as_java_s() {
        // 0.1 to f64 then f32 happens to agree, but parsing must not depend on that.
        assert_eq!(
            float(&serde_json::from_str("0.1").unwrap(), "x").unwrap(),
            0.1f32
        );
        assert_eq!(
            float(&serde_json::from_str("1e-3").unwrap(), "x").unwrap(),
            0.001f32
        );
    }

    #[test]
    fn what_java_refuses() {
        assert!(Model::parse(br##"{"elements": [{"from": [0,0,0], "to": [40,0,0], "faces": {"up": {"texture": "#a"}}}]}"##).is_err());
        assert!(
            Model::parse(br#"{"elements": [{"from": [0,0,0], "to": [1,1,1], "faces": {}}]}"#)
                .is_err()
        );
        assert!(Model::parse(br##"{"elements": [{"from": [0,0,0], "to": [1,1,1], "faces": {"up": {"texture": "#a", "rotation": 45}}}]}"##).is_err());
        assert!(BlockState::parse(br#"{}"#).is_err());
    }

    #[test]
    fn blockstates_variants_and_multipart() {
        let state = BlockState::parse(
            br#"{"multipart": [
                {"apply": {"model": "block/fence_post"}},
                {"when": {"OR": [{"north": "true"}, {"facing": "east|!west", "lit": true}]},
                 "apply": [{"model": "block/a", "y": 90, "uvlock": true}, {"model": "block/b", "weight": 3}]}]}"#,
        )
        .unwrap();
        assert_eq!(state.multipart.len(), 2);
        assert!(state.multipart[0].when.is_none());
        let part = &state.multipart[1];
        assert_eq!(part.apply[0].variant.y, Quadrant(1));
        assert!(part.apply[0].variant.uvlock);
        assert_eq!(part.apply[1].weight, 3);
        let Some(Condition::Or(terms)) = &part.when else {
            panic!()
        };
        assert_eq!(
            terms[1],
            Condition::Properties(vec![
                (
                    "facing".into(),
                    vec![("east".into(), false), ("west".into(), true)]
                ),
                ("lit".into(), vec![("true".into(), false)]),
            ])
        );
    }
}
