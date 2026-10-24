//! Baking models into quads as 1.21.11 does (DESIGN.md §3.7): resolving a model's parents and
//! texture variables, turning each element face into a quad (default UVs, face rotation, element
//! rotation, the variant's orientation, uvlock, winding), keeping unculled quads first and then
//! culled ones by face. Exact to the game's floats: the matrix work is `joml`'s.
//!
//! Quads keep sprite-local UVs (0–1 across the sprite) and the texture's id, not atlas UVs: our
//! renderer maps them through the atlas, frame by frame.

use std::collections::{BTreeMap, HashMap};

use crate::joml::{self, Mat3, Mat4, Quat, Vec3};
use crate::model::{Axis, Direction, Element, Face, Id, Model, Quadrant, RotationValue, Slot};

/// What baking a face produces.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    /// Corners in block space (0–1), in winding order.
    pub positions: [Vec3; 4],
    /// Sprite-local (u, v) per corner.
    pub uvs: [[f32; 2]; 4],
    /// The texture's id, or `None` for an unresolved variable (Java's missing texture).
    pub texture: Option<Id>,
    pub tint: i32,
    /// The direction the quad faces, from its geometry (`UP` if it has none).
    pub facing: Direction,
    /// Culled when this neighbour is solid; `None`: never culled.
    pub cull: Option<Direction>,
    pub shade: bool,
    pub light_emission: u8,
}

/// A model's quads, unculled first, then culled ones grouped north, south, east, west, up, down,
/// as Java's `QuadCollection` lists them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Baked {
    pub quads: Vec<Quad>,
    pub ambient_occlusion: bool,
}

// ---- The cube's 48 symmetries (`OctahedralGroup`) ----

/// A symmetry: a permutation of the axes (`SymmetricGroup3`) and which axes it inverts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Octahedral {
    perm: [usize; 3],
    invert: [bool; 3],
}

const P123: [usize; 3] = [0, 1, 2];
const P213: [usize; 3] = [1, 0, 2];
const P132: [usize; 3] = [0, 2, 1];
const P321: [usize; 3] = [2, 1, 0];

impl Octahedral {
    pub const IDENTITY: Octahedral = Octahedral {
        perm: P123,
        invert: [false; 3],
    };

    const fn new(perm: [usize; 3], x: bool, y: bool, z: bool) -> Self {
        Self {
            perm,
            invert: [x, y, z],
        }
    }

    /// Java's `compose`, as its Cayley table defines it.
    pub fn compose(self, other: Octahedral) -> Octahedral {
        Octahedral {
            perm: std::array::from_fn(|i| other.perm[self.perm[i]]),
            invert: std::array::from_fn(|axis| self.invert[axis] ^ other.invert[self.perm[axis]]),
        }
    }

    /// Quarter turns about x, y and z (`Quadrant.rotationX` …).
    fn about(axis: Axis, quarter: Quadrant) -> Octahedral {
        match (axis, quarter.0) {
            (_, 0) => Self::IDENTITY,
            (Axis::X, 1) => Self::new(P132, false, false, true),
            (Axis::X, 2) => Self::new(P123, false, true, true),
            (Axis::X, 3) => Self::new(P132, false, true, false),
            (Axis::Y, 1) => Self::new(P321, true, false, false),
            (Axis::Y, 2) => Self::new(P123, true, false, true),
            (Axis::Y, 3) => Self::new(P321, false, false, true),
            (Axis::Z, 1) => Self::new(P213, false, true, false),
            (Axis::Z, 2) => Self::new(P123, true, true, false),
            (Axis::Z, 3) => Self::new(P213, true, false, false),
            _ => unreachable!("quadrants are 0 to 3"),
        }
    }

    /// `Quadrant.fromXYZAngles`.
    pub fn from_xyz(x: Quadrant, y: Quadrant, z: Quadrant) -> Octahedral {
        Self::about(Axis::Z, z).compose(Self::about(Axis::Y, y).compose(Self::about(Axis::X, x)))
    }

    /// Its matrix, built as Java builds it (scaling × permutation, in floats, so the zeros keep
    /// Java's signs).
    pub fn matrix(&self) -> Mat3 {
        let mut permutation = Mat3 { m: [[0.0; 3]; 3] };
        for (row, &col) in self.perm.iter().enumerate() {
            permutation.m[col][row] = 1.0;
        }
        let sign = |i: bool| if i { -1.0 } else { 1.0 };
        Mat3::scaling([
            sign(self.invert[0]),
            sign(self.invert[1]),
            sign(self.invert[2]),
        ])
        .mul(&permutation)
    }
}

// ---- Model states (`BlockModelRotation`, `BlockMath`) ----

/// An orientation and its per-face UV transforms, as `BlockModelRotation` precomputes them.
#[derive(Clone, Debug)]
pub struct ModelState {
    /// `None` for the identity, which Java skips rather than applies.
    transformation: Option<Mat4>,
    inverse_face: [Mat4; 6],
    uvlock: bool,
}

/// `BlockMath`'s per-face transforms from face-local UV space, and back.
struct UvTransforms {
    local_to_global: [Option<Mat4>; 6],
    global_to_local: [Option<Mat4>; 6],
}

impl UvTransforms {
    fn new() -> Self {
        let pi = joml::PI_F;
        let half = (std::f64::consts::PI / 2.0) as f32;
        let rotation = |q: Quat| Some(Mat4::identity().rotate(&q));
        // Indexed by Direction: down, up, north, south, west, east; south is the identity.
        let local_to_global = [
            rotation(Quat::IDENTITY.rotate_x(half)),
            rotation(Quat::IDENTITY.rotate_x(-half)),
            rotation(Quat::IDENTITY.rotate_y(pi)),
            None,
            rotation(Quat::IDENTITY.rotate_y(-half)),
            rotation(Quat::IDENTITY.rotate_y(half)),
        ];
        let global_to_local = local_to_global.map(|m| {
            m.map(|m| {
                let inverse = m.invert_affine();
                assert!(inverse.is_finite());
                inverse
            })
        });
        Self {
            local_to_global,
            global_to_local,
        }
    }

    /// `getFaceTransformation`.
    fn face(&self, transformation: &mut Mat4, direction: Direction) -> Mat4 {
        if transformation.is_identity() {
            return *transformation;
        }
        let composed = match &self.local_to_global[direction as usize] {
            Some(m) => transformation.mul(m),
            None => transformation.mul(&Mat4::identity()),
        };
        let v = composed.transform_direction([0.0, 0.0, 1.0]);
        let nearest = approximate_nearest(v);
        match &self.global_to_local[nearest as usize] {
            Some(m) => m.mul(&composed),
            None => Mat4::identity().mul(&composed),
        }
    }
}

impl ModelState {
    pub fn new(orientation: Octahedral, uvlock: bool) -> Self {
        let uv = UvTransforms::new();
        let mut transformation =
            (orientation != Octahedral::IDENTITY).then(|| Mat4::from_mat3(&orientation.matrix()));
        let mut matrix = transformation.unwrap_or_default();
        let inverse_face =
            Direction::ALL.map(|direction| uv.face(&mut matrix, direction).invert_affine());
        // `is_identity` above settled the transformation's properties, as Java's does.
        if let Some(t) = transformation.as_mut() {
            *t = matrix;
        }
        Self {
            transformation,
            inverse_face,
            uvlock,
        }
    }

    fn inverse_face(&self, direction: Direction) -> Option<Mat4> {
        self.uvlock.then(|| self.inverse_face[direction as usize])
    }
}

/// `Direction.getApproximateNearest`: the direction with the largest dot product (north on no
/// positive one, as Java starts from north with `Float.MIN_VALUE`).
fn approximate_nearest(v: Vec3) -> Direction {
    let mut best = Direction::North;
    let mut best_dot = f32::from_bits(1); // Float.MIN_VALUE, the smallest positive float
    for direction in Direction::ALL {
        let n = direction.normal().map(|c| c as f32);
        let d = v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
        if d > best_dot {
            best_dot = d;
            best = direction;
        }
    }
    best
}

// ---- Resolution (`ModelDiscovery`, `ResolvedModel`, `TextureSlots`) ----

/// Every model file the pack stack has, by id, resolved on demand with Java's rules: a missing
/// model is the missing model, a parent cycle leaves its models out.
pub struct Models {
    files: HashMap<Id, Model>,
    pub notes: Vec<String>,
}

pub const MISSING_MODEL: &str = "minecraft:builtin/missing";
pub const MISSING_TEXTURE: &str = "minecraft:missingno";

impl Models {
    pub fn new(files: HashMap<Id, Model>) -> Self {
        Self {
            files,
            notes: Vec::new(),
        }
    }

    /// The model and its ancestors, nearest first; `None` if the chain loops.
    fn chain(&mut self, id: &str) -> Option<Vec<Model>> {
        let mut chain = Vec::new();
        let mut seen = Vec::new();
        let mut current = id.to_owned();
        loop {
            if seen.contains(&current) {
                self.notes
                    .push(format!("Model {id} ignored due to cyclic dependency"));
                return None;
            }
            seen.push(current.clone());
            let model = match self.files.get(&current) {
                Some(model) if current != MISSING_MODEL => model.clone(),
                _ => {
                    if current != MISSING_MODEL {
                        self.notes.push(format!("Missing block model: {current}"));
                    }
                    chain.push(missing_model());
                    return Some(chain);
                }
            };
            let parent = model.parent.clone();
            chain.push(model);
            match parent {
                Some(parent) => current = parent,
                None => return Some(chain),
            }
        }
    }

    /// Bakes `id` in `state`; a model in a parent cycle bakes as the missing model.
    pub fn bake(&mut self, id: &str, state: &ModelState) -> Baked {
        let chain = self.chain(id).unwrap_or_else(|| vec![missing_model()]);
        let elements = chain
            .iter()
            .find_map(|m| m.elements.clone())
            .unwrap_or_default();
        let ambient_occlusion = chain
            .iter()
            .find_map(|m| m.ambient_occlusion)
            .unwrap_or(true);
        let textures = resolve_textures(&chain, id, &mut self.notes);
        Baked {
            quads: bake_elements(&elements, &textures, state),
            ambient_occlusion,
        }
    }
}

/// Java's missing model: a full cube, every face the missing texture.
fn missing_model() -> Model {
    let face = |d: Direction| {
        (
            d,
            Face {
                cull: Some(d),
                tint: -1,
                texture: "missingno".into(),
                uv: Some([0.0, 0.0, 16.0, 16.0]),
                rotation: Quadrant(0),
            },
        )
    };
    Model {
        elements: Some(vec![Element {
            from: [0.0; 3],
            to: [16.0; 3],
            rotation: None,
            shade: true,
            light_emission: 0,
            faces: Direction::ALL.map(face).to_vec(),
        }]),
        textures: vec![
            ("particle".into(), Slot::Reference("missingno".into())),
            ("missingno".into(), Slot::Texture(MISSING_TEXTURE.into())),
        ],
        ..Model::default()
    }
}

/// Texture variables: the root's first, each child's over them, then references followed until
/// nothing more resolves (`TextureSlots.Resolver`).
fn resolve_textures(chain: &[Model], id: &str, notes: &mut Vec<String>) -> BTreeMap<String, Id> {
    let mut values = BTreeMap::new();
    let mut references = BTreeMap::new();
    for model in chain.iter().rev() {
        for (name, slot) in &model.textures {
            match slot {
                Slot::Texture(texture) => {
                    references.remove(name);
                    values.insert(name.clone(), texture.clone());
                }
                Slot::Reference(target) => {
                    values.remove(name);
                    references.insert(name.clone(), target.clone());
                }
            }
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        let resolvable: Vec<(String, Id)> = references
            .iter()
            .filter_map(|(name, target)| values.get(target).map(|t| (name.clone(), t.clone())))
            .collect();
        for (name, texture) in resolvable {
            references.remove(&name);
            values.insert(name, texture);
            changed = true;
        }
    }
    if !references.is_empty() {
        let list: Vec<String> = references
            .iter()
            .map(|(n, t)| format!("#{n}-> #{t}"))
            .collect();
        notes.push(format!(
            "Unresolved texture references in {id}: {}",
            list.join(", ")
        ));
    }
    values
}

// ---- Baking (`SimpleUnbakedGeometry`, `FaceBakery`) ----

fn bake_elements(
    elements: &[Element],
    textures: &BTreeMap<String, Id>,
    state: &ModelState,
) -> Vec<Quad> {
    let mut unculled = Vec::new();
    let mut culled: [Vec<Quad>; 6] = Default::default();
    for element in elements {
        let (from, to) = (element.from, element.to);
        // A flat element only has faces along its flat axis.
        let mut x = true;
        let mut y = true;
        let mut z = true;
        if from[0] == to[0] {
            y = false;
            z = false;
        }
        if from[1] == to[1] {
            x = false;
            z = false;
        }
        if from[2] == to[2] {
            x = false;
            y = false;
        }
        if !(x || y || z) {
            continue;
        }
        let rotation = element.rotation.as_ref().map(element_transform);
        for (direction, face) in &element.faces {
            let keep = match direction.axis() {
                Axis::X => x,
                Axis::Y => y,
                Axis::Z => z,
            };
            if !keep {
                continue;
            }
            let texture = face.texture.strip_prefix('#').unwrap_or(&face.texture);
            let mut quad = bake_face(element, face, *direction, rotation.as_ref(), state);
            quad.texture = textures.get(texture).cloned();
            match face.cull {
                None => unculled.push(quad),
                Some(cull) => {
                    let transform = state.transformation.unwrap_or_default();
                    let rotated = approximate_nearest(
                        transform.transform_direction(cull.normal().map(|c| c as f32)),
                    );
                    quad.cull = Some(rotated);
                    culled[rotated as usize].push(quad);
                }
            }
        }
    }
    // North, south, east, west, up, down.
    for direction in [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ] {
        unculled.append(&mut culled[direction as usize]);
    }
    unculled
}

/// An element rotation's origin and matrix (`BlockElementRotation.computeTransform`).
fn element_transform(rotation: &crate::model::ElementRotation) -> (Vec3, Mat4) {
    let degrees = (std::f64::consts::PI / 180.0) as f32;
    // `angle == 0.0` as Java's, true for -0.0 too, rather than a float pattern.
    #[allow(clippy::redundant_guards)]
    let mut matrix = match rotation.value {
        RotationValue::Euler([x, y, z]) => {
            Mat4::rotation_zyx(z * degrees, y * degrees, x * degrees)
        }
        RotationValue::Axis(_, angle) if angle == 0.0 => Mat4::identity(),
        RotationValue::Axis(axis, angle) => {
            let unit = match axis {
                Axis::X => [1.0, 0.0, 0.0],
                Axis::Y => [0.0, 1.0, 0.0],
                Axis::Z => [0.0, 0.0, 1.0],
            };
            Mat4::rotation_axis(angle * degrees, unit)
        }
    };
    if rotation.rescale && !matrix.is_identity() {
        let factor = |unit: Vec3| {
            let v = matrix.transform_direction(unit);
            1.0 / v[0].abs().max(v[1].abs()).max(v[2].abs())
        };
        let scale = [
            factor([1.0, 0.0, 0.0]),
            factor([0.0, 1.0, 0.0]),
            factor([0.0, 0.0, 1.0]),
        ];
        matrix = matrix.scale(scale);
    }
    (rotation.origin, matrix)
}

/// Which bound each corner takes, per face (`FaceInfo`): (x, y, z), false for min, true for max.
fn corners(direction: Direction) -> [[bool; 3]; 4] {
    const F: bool = false;
    const T: bool = true;
    match direction {
        Direction::Down => [[F, F, T], [F, F, F], [T, F, F], [T, F, T]],
        Direction::Up => [[F, T, F], [F, T, T], [T, T, T], [T, T, F]],
        Direction::North => [[T, T, F], [T, F, F], [F, F, F], [F, T, F]],
        Direction::South => [[F, T, T], [F, F, T], [T, F, T], [T, T, T]],
        Direction::West => [[F, T, F], [F, F, F], [F, F, T], [F, T, T]],
        Direction::East => [[T, T, T], [T, F, T], [T, F, F], [T, T, F]],
    }
}

/// `defaultFaceUV`, in sixteenths.
fn default_uv(from: Vec3, to: Vec3, direction: Direction) -> [f32; 4] {
    match direction {
        Direction::Down => [from[0], 16.0 - to[2], to[0], 16.0 - from[2]],
        Direction::Up => [from[0], from[2], to[0], to[2]],
        Direction::North => [16.0 - to[0], 16.0 - to[1], 16.0 - from[0], 16.0 - from[1]],
        Direction::South => [from[0], 16.0 - to[1], to[0], 16.0 - from[1]],
        Direction::West => [from[2], 16.0 - to[1], to[2], 16.0 - from[1]],
        Direction::East => [16.0 - to[2], 16.0 - to[1], 16.0 - from[2], 16.0 - from[1]],
    }
}

fn bake_face(
    element: &Element,
    face: &Face,
    direction: Direction,
    rotation: Option<&(Vec3, Mat4)>,
    state: &ModelState,
) -> Quad {
    let uv = face
        .uv
        .unwrap_or_else(|| default_uv(element.from, element.to, direction));
    let mut inverse_face = state.inverse_face(direction);
    let uv_identity = inverse_face.as_mut().is_none_or(|m| m.is_identity());
    let mut positions = [[0.0f32; 3]; 4];
    let mut uvs = [[0.0f32; 2]; 4];
    for (i, corner) in corners(direction).iter().enumerate() {
        let mut v: Vec3 = std::array::from_fn(|axis| {
            let bound = if corner[axis] {
                element.to[axis]
            } else {
                element.from[axis]
            };
            bound / 16.0
        });
        if let Some((origin, matrix)) = rotation {
            v = rotate_about(v, *origin, matrix);
        }
        if let Some(transformation) = &state.transformation {
            v = rotate_about(v, [0.5; 3], transformation);
        }
        // `BlockElementFace.getU/getV`: the face rotation shifts which corner's UV each takes.
        let corner_uv = (i + face.rotation.0 as usize) % 4;
        let u = (if corner_uv == 0 || corner_uv == 1 {
            uv[0]
        } else {
            uv[2]
        }) / 16.0;
        let w = (if corner_uv == 0 || corner_uv == 3 {
            uv[1]
        } else {
            uv[3]
        }) / 16.0;
        uvs[i] = if uv_identity {
            [u, w]
        } else {
            let t = inverse_face
                .unwrap()
                .transform_position([u - 0.5, w - 0.5, 0.0]);
            [t[0] + 0.5, t[1] + 0.5]
        };
        positions[i] = v;
    }

    let n = joml::normal(positions[0], positions[1], positions[2]);
    let facing = closest_direction(n);
    if rotation.is_none()
        && let Some(facing) = facing
    {
        recalculate_winding(&mut positions, &mut uvs, facing);
    }
    Quad {
        positions,
        uvs,
        texture: None,
        tint: face.tint,
        facing: facing.unwrap_or(Direction::Up),
        cull: None,
        shade: element.shade,
        light_emission: element.light_emission,
    }
}

/// `rotateVertexBy`: about `origin`, so `(v − origin)` then back, rounding as Java's does.
fn rotate_about(v: Vec3, origin: Vec3, matrix: &Mat4) -> Vec3 {
    let shifted = [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];
    let turned = matrix.transform_position(shifted);
    [
        turned[0] + origin[0],
        turned[1] + origin[1],
        turned[2] + origin[2],
    ]
}

/// `findClosestDirection`: the face direction the normal points most along, if any positively.
fn closest_direction(n: Vec3) -> Option<Direction> {
    if !n.iter().all(|c| c.is_finite()) {
        return None;
    }
    let mut best = None;
    let mut best_dot = 0.0f32;
    for direction in Direction::ALL {
        let d = joml::dot(n, direction.normal().map(|c| c as f32));
        if d >= 0.0 && d > best_dot {
            best_dot = d;
            best = Some(direction);
        }
    }
    best
}

/// `recalculateWinding`: reorders the corners to `facing`'s canonical order.
fn recalculate_winding(positions: &mut [Vec3; 4], uvs: &mut [[f32; 2]; 4], facing: Direction) {
    let mut min = [999.0f32; 3];
    let mut max = [-999.0f32; 3];
    for p in positions.iter() {
        for axis in 0..3 {
            if p[axis] < min[axis] {
                min[axis] = p[axis];
            }
            if p[axis] > max[axis] {
                max[axis] = p[axis];
            }
        }
    }
    for (i, corner) in corners(facing).iter().enumerate() {
        let want: Vec3 =
            std::array::from_fn(|axis| if corner[axis] { max[axis] } else { min[axis] });
        let found = (i..4)
            .find(|&j| positions[j] == want)
            .expect("Can't find vertex to swap");
        positions.swap(i, found);
        uvs.swap(i, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;

    fn models(files: &[(&str, &str)]) -> Models {
        Models::new(
            files
                .iter()
                .map(|(id, json)| (id.to_string(), Model::parse(json.as_bytes()).unwrap()))
                .collect(),
        )
    }

    #[test]
    fn a_cube_bakes_to_six_culled_faces_in_java_s_order() {
        let mut models = models(&[
            (
                "minecraft:block/cube_all",
                r##"{"textures": {"particle": "#all"}, "elements": [{"from": [0,0,0], "to": [16,16,16], "faces": {
                    "down": {"texture": "#all", "cullface": "down"}, "up": {"texture": "#all", "cullface": "up"},
                    "north": {"texture": "#all", "cullface": "north"}, "south": {"texture": "#all", "cullface": "south"},
                    "west": {"texture": "#all", "cullface": "west"}, "east": {"texture": "#all", "cullface": "east"}}}]}"##,
            ),
            (
                "minecraft:block/stone",
                r#"{"parent": "block/cube_all", "textures": {"all": "block/stone"}}"#,
            ),
        ]);
        let baked = models.bake(
            "minecraft:block/stone",
            &ModelState::new(Octahedral::IDENTITY, false),
        );
        let order: Vec<Direction> = baked.quads.iter().map(|q| q.cull.unwrap()).collect();
        use Direction::*;
        assert_eq!(order, [North, South, East, West, Up, Down]);
        assert!(
            baked
                .quads
                .iter()
                .all(|q| q.texture.as_deref() == Some("minecraft:block/stone"))
        );
        let up = &baked.quads[4];
        assert_eq!(up.facing, Up);
        assert_eq!(up.positions[0], [0.0, 1.0, 0.0]);
        assert_eq!(up.uvs[0], [0.0, 0.0]);
        assert!(baked.ambient_occlusion);
    }

    #[test]
    fn a_quarter_turn_about_y_moves_north_to_east() {
        let mut models = models(&[(
            "minecraft:block/x",
            r##"{"textures": {"a": "block/a"}, "elements": [{"from": [0,0,0], "to": [16,16,16],
                "faces": {"north": {"texture": "#a", "cullface": "north"}}}]}"##,
        )]);
        let y90 = Octahedral::from_xyz(Quadrant(0), Quadrant(1), Quadrant(0));
        let baked = models.bake("minecraft:block/x", &ModelState::new(y90, false));
        assert_eq!(baked.quads[0].cull, Some(Direction::East));
        assert_eq!(baked.quads[0].facing, Direction::East);
    }

    #[test]
    fn missing_models_and_cycles_bake_as_the_missing_cube() {
        let mut models = models(&[
            ("minecraft:block/a", r#"{"parent": "block/b"}"#),
            ("minecraft:block/b", r#"{"parent": "block/a"}"#),
        ]);
        let state = ModelState::new(Octahedral::IDENTITY, false);
        for id in ["minecraft:block/a", "minecraft:block/nowhere"] {
            let baked = models.bake(id, &state);
            assert_eq!(baked.quads.len(), 6);
            assert!(
                baked
                    .quads
                    .iter()
                    .all(|q| q.texture.as_deref() == Some(MISSING_TEXTURE))
            );
        }
        assert!(models.notes.iter().any(|n| n.contains("cyclic")));
    }

    #[test]
    fn composition_matches_java_s_cayley_rule() {
        // Four quarter turns about y come back to the identity; x 90 then y 90 isn't y then x.
        let y = |q| Octahedral::from_xyz(Quadrant(0), Quadrant(q), Quadrant(0));
        assert_eq!(
            y(1).compose(y(1)).compose(y(1)).compose(y(1)),
            Octahedral::IDENTITY
        );
        assert_eq!(y(1).compose(y(1)), y(2));
        let xy = Octahedral::from_xyz(Quadrant(1), Quadrant(1), Quadrant(0));
        let yx = Octahedral::about(Axis::X, Quadrant(1))
            .compose(Octahedral::about(Axis::Y, Quadrant(1)));
        assert_ne!(xy, yx);
    }
}
