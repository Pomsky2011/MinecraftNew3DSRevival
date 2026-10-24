//! The slice of JOML 1.10.8 that 1.21.11's model baking uses, kept to its operation order so
//! results match the game's to the bit (DESIGN.md §3.7): matrices with JOML's property flags
//! (which pick the arithmetic path), quaternion turns, and its float trigonometry.
//!
//! As the game runs JOML: `joml.useMathFma` off, so `fma(a, b, c)` is `a * b + c`, rounded twice;
//! `joml.fastmath` off, so `sin`/`cos`/`sqrt` are Java's in double, rounded to float.
//!
//! Derived from JOML (https://github.com/JOML-CI/JOML), under its licence:
//!
//! The MIT License
//!
//! Copyright (c) 2015-2024 Richard Greenlees
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

#![allow(clippy::too_many_arguments)]

pub const PERSPECTIVE: u8 = 1;
pub const AFFINE: u8 = 2;
pub const IDENTITY: u8 = 4;
pub const TRANSLATION: u8 = 8;
pub const ORTHONORMAL: u8 = 16;

pub const PI_F: f32 = std::f64::consts::PI as f32;
const PI_TIMES_2_F: f32 = PI_F * 2.0;
const PI_OVER_2_F: f32 = (std::f64::consts::PI * 0.5) as f32;

/// JOML's `fma` with `useMathFma` off: two roundings.
#[inline]
fn fma(a: f32, b: f32, c: f32) -> f32 {
    a * b + c
}

pub fn sin(rad: f32) -> f32 {
    (rad as f64).sin() as f32
}

pub fn sqrt(r: f32) -> f32 {
    (r as f64).sqrt() as f32
}

fn invsqrt(r: f32) -> f32 {
    1.0 / (r as f64).sqrt() as f32
}

/// `cosFromSin`: the cosine from a sine, its sign from the angle's quadrant.
pub fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    let cos = sqrt(1.0 - sin * sin);
    let a = angle + PI_OVER_2_F;
    let mut b = a - ((a / PI_TIMES_2_F) as i32) as f32 * PI_TIMES_2_F;
    if (b as f64) < 0.0 {
        b += PI_TIMES_2_F;
    }
    if b >= PI_F { -cos } else { cos }
}

/// A 3-vector.
pub type Vec3 = [f32; 3];

/// A column-major 4×4 matrix, `m[col][row]` as JOML's `mColRow`, with its property flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
    pub properties: u8,
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat4 {
    /// `new Matrix4f()`.
    pub fn identity() -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            properties: IDENTITY | AFFINE | TRANSLATION | ORTHONORMAL,
        }
    }

    /// `new Matrix4f(Matrix3fc)`: marked affine only, even when it's the identity.
    pub fn from_mat3(m: &Mat3) -> Self {
        let c = &m.m;
        Self {
            m: [
                [c[0][0], c[0][1], c[0][2], 0.0],
                [c[1][0], c[1][1], c[1][2], 0.0],
                [c[2][0], c[2][1], c[2][2], 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            properties: AFFINE,
        }
    }

    /// `determineProperties`.
    pub fn determine_properties(&mut self) {
        let m = &self.m;
        let mut properties = 0;
        if m[0][3] == 0.0 && m[1][3] == 0.0 {
            if m[2][3] == 0.0 && m[3][3] == 1.0 {
                properties |= AFFINE;
                if m[0][0] == 1.0
                    && m[0][1] == 0.0
                    && m[0][2] == 0.0
                    && m[1][0] == 0.0
                    && m[1][1] == 1.0
                    && m[1][2] == 0.0
                    && m[2][0] == 0.0
                    && m[2][1] == 0.0
                    && m[2][2] == 1.0
                {
                    properties |= TRANSLATION | ORTHONORMAL;
                    if m[3][0] == 0.0 && m[3][1] == 0.0 && m[3][2] == 0.0 {
                        properties |= IDENTITY;
                    }
                }
            } else if m[0][1] == 0.0
                && m[0][2] == 0.0
                && m[1][0] == 0.0
                && m[1][2] == 0.0
                && m[2][0] == 0.0
                && m[2][1] == 0.0
                && m[3][0] == 0.0
                && m[3][1] == 0.0
                && m[3][3] == 0.0
            {
                properties |= PERSPECTIVE;
            }
        }
        self.properties = properties;
    }

    /// `MatrixUtil.isIdentity`: the flag, else determining the properties (which, as in Java,
    /// sticks to the matrix).
    pub fn is_identity(&mut self) -> bool {
        if self.properties & IDENTITY != 0 {
            return true;
        }
        self.determine_properties();
        self.properties & IDENTITY != 0
    }

    /// `this.mul(right)`, by JOML's paths.
    pub fn mul(&self, right: &Mat4) -> Mat4 {
        if self.properties & IDENTITY != 0 {
            return *right;
        }
        if right.properties & IDENTITY != 0 {
            return *self;
        }
        if self.properties & TRANSLATION != 0 && right.properties & AFFINE != 0 {
            return self.mul_translation_affine(right);
        }
        if self.properties & AFFINE != 0 && right.properties & AFFINE != 0 {
            return self.mul_affine(right);
        }
        assert!(
            self.properties & PERSPECTIVE == 0,
            "perspective matrices aren't used in baking"
        );
        if right.properties & AFFINE != 0 {
            return self.mul_affine_r(right);
        }
        self.mul0(right)
    }

    fn mul_translation_affine(&self, r: &Mat4) -> Mat4 {
        let (a, b) = (&self.m, &r.m);
        Mat4 {
            m: [
                [b[0][0], b[0][1], b[0][2], a[0][3]],
                [b[1][0], b[1][1], b[1][2], a[1][3]],
                [b[2][0], b[2][1], b[2][2], a[2][3]],
                [
                    b[3][0] + a[3][0],
                    b[3][1] + a[3][1],
                    b[3][2] + a[3][2],
                    a[3][3],
                ],
            ],
            properties: AFFINE | (r.properties & ORTHONORMAL),
        }
    }

    fn mul_affine(&self, r: &Mat4) -> Mat4 {
        let (a, b) = (&self.m, &r.m);
        let mut m = [[0.0; 4]; 4];
        for col in 0..3 {
            for row in 0..3 {
                m[col][row] = fma(
                    a[0][row],
                    b[col][0],
                    fma(a[1][row], b[col][1], a[2][row] * b[col][2]),
                );
            }
            m[col][3] = a[col][3];
        }
        for row in 0..3 {
            m[3][row] = fma(
                a[0][row],
                b[3][0],
                fma(a[1][row], b[3][1], fma(a[2][row], b[3][2], a[3][row])),
            );
        }
        m[3][3] = a[3][3];
        Mat4 {
            m,
            properties: AFFINE | (self.properties & r.properties & ORTHONORMAL),
        }
    }

    fn mul_affine_r(&self, r: &Mat4) -> Mat4 {
        let (a, b) = (&self.m, &r.m);
        let mut m = [[0.0; 4]; 4];
        for col in 0..3 {
            for row in 0..4 {
                m[col][row] = fma(
                    a[0][row],
                    b[col][0],
                    fma(a[1][row], b[col][1], a[2][row] * b[col][2]),
                );
            }
        }
        for row in 0..4 {
            m[3][row] = fma(
                a[0][row],
                b[3][0],
                fma(a[1][row], b[3][1], fma(a[2][row], b[3][2], a[3][row])),
            );
        }
        Mat4 {
            m,
            properties: self.properties & !(IDENTITY | PERSPECTIVE | TRANSLATION | ORTHONORMAL),
        }
    }

    fn mul0(&self, r: &Mat4) -> Mat4 {
        let (a, b) = (&self.m, &r.m);
        let mut m = [[0.0; 4]; 4];
        for col in 0..4 {
            for row in 0..4 {
                m[col][row] = fma(
                    a[0][row],
                    b[col][0],
                    fma(
                        a[1][row],
                        b[col][1],
                        fma(a[2][row], b[col][2], a[3][row] * b[col][3]),
                    ),
                );
            }
        }
        Mat4 { m, properties: 0 }
    }

    /// `new Matrix4f().rotation(quat)` and `rotate(quat)`, by JOML's paths.
    pub fn rotate(&self, q: &Quat) -> Mat4 {
        let rm = q.rotation_terms();
        if self.properties & IDENTITY != 0 {
            let mut out = Mat4::identity();
            for col in 0..3 {
                out.m[col][..3].copy_from_slice(&rm[col]);
            }
            out.properties = AFFINE | ORTHONORMAL;
            return out;
        }
        let a = &self.m;
        let mut m = self.m;
        let properties = self.properties & !(PERSPECTIVE | IDENTITY | TRANSLATION);
        if self.properties & TRANSLATION != 0 {
            for col in 0..3 {
                m[col] = [rm[col][0], rm[col][1], rm[col][2], 0.0];
            }
        } else if self.properties & AFFINE != 0 {
            for col in 0..3 {
                for row in 0..3 {
                    m[col][row] =
                        a[0][row] * rm[col][0] + a[1][row] * rm[col][1] + a[2][row] * rm[col][2];
                }
                m[col][3] = 0.0;
            }
        } else {
            for col in 0..3 {
                for row in 0..4 {
                    m[col][row] = fma(
                        a[0][row],
                        rm[col][0],
                        fma(a[1][row], rm[col][1], a[2][row] * rm[col][2]),
                    );
                }
            }
        }
        Mat4 { m, properties }
    }

    /// `new Matrix4f().rotation(angle, axis)` for a unit axis vector.
    pub fn rotation_axis(angle: f32, [x, y, z]: Vec3) -> Mat4 {
        let abs_one = |v: f32| v.to_bits() & 0x7FFF_FFFF == 0x3F80_0000;
        let mut out = Mat4::identity();
        if y == 0.0 && z == 0.0 && abs_one(x) {
            let a = x * angle;
            let (s, c) = (sin(a), cos_from_sin(sin(a), a));
            out.m[1][1] = c;
            out.m[1][2] = s;
            out.m[2][1] = -s;
            out.m[2][2] = c;
        } else if x == 0.0 && z == 0.0 && abs_one(y) {
            let a = y * angle;
            let (s, c) = (sin(a), cos_from_sin(sin(a), a));
            out.m[0][0] = c;
            out.m[0][2] = -s;
            out.m[2][0] = s;
            out.m[2][2] = c;
        } else if x == 0.0 && y == 0.0 && abs_one(z) {
            let a = z * angle;
            let (s, c) = (sin(a), cos_from_sin(sin(a), a));
            out.m[0][0] = c;
            out.m[0][1] = s;
            out.m[1][0] = -s;
            out.m[1][1] = c;
        } else {
            let s = sin(angle);
            let c = cos_from_sin(s, angle);
            let k = 1.0 - c;
            let (xy, xz, yz) = (x * y, x * z, y * z);
            out.m[0][0] = c + x * x * k;
            out.m[1][0] = xy * k - z * s;
            out.m[2][0] = xz * k + y * s;
            out.m[0][1] = xy * k + z * s;
            out.m[1][1] = c + y * y * k;
            out.m[2][1] = yz * k - x * s;
            out.m[0][2] = xz * k - y * s;
            out.m[1][2] = yz * k + x * s;
            out.m[2][2] = c + z * z * k;
        }
        out.properties = AFFINE | ORTHONORMAL;
        out
    }

    /// `new Matrix4f().rotationZYX(z, y, x)`.
    pub fn rotation_zyx(angle_z: f32, angle_y: f32, angle_x: f32) -> Mat4 {
        let sin_x = sin(angle_x);
        let cos_x = cos_from_sin(sin_x, angle_x);
        let sin_y = sin(angle_y);
        let cos_y = cos_from_sin(sin_y, angle_y);
        let sin_z = sin(angle_z);
        let cos_z = cos_from_sin(sin_z, angle_z);
        let nm20 = cos_z * sin_y;
        let nm21 = sin_z * sin_y;
        Mat4 {
            m: [
                [cos_z * cos_y, sin_z * cos_y, -sin_y, 0.0],
                [
                    -sin_z * cos_x + nm20 * sin_x,
                    cos_z * cos_x + nm21 * sin_x,
                    cos_y * sin_x,
                    0.0,
                ],
                [
                    -sin_z * -sin_x + nm20 * cos_x,
                    cos_z * -sin_x + nm21 * cos_x,
                    cos_y * cos_x,
                    0.0,
                ],
                [0.0, 0.0, 0.0, 1.0],
            ],
            properties: AFFINE | ORTHONORMAL,
        }
    }

    /// `scale(x, y, z)`, by JOML's paths.
    pub fn scale(&self, [x, y, z]: Vec3) -> Mat4 {
        let abs_one = |v: f32| v.to_bits() & 0x7FFF_FFFF == 0x3F80_0000;
        let one = abs_one(x) && abs_one(y) && abs_one(z);
        if self.properties & IDENTITY != 0 {
            let mut out = Mat4::identity();
            out.m[0][0] = x;
            out.m[1][1] = y;
            out.m[2][2] = z;
            out.properties = AFFINE | if one { ORTHONORMAL } else { 0 };
            return out;
        }
        let mut out = *self;
        for (col, factor) in [x, y, z].into_iter().enumerate() {
            for row in 0..4 {
                out.m[col][row] = self.m[col][row] * factor;
            }
        }
        out.properties = self.properties
            & !(PERSPECTIVE | IDENTITY | TRANSLATION | if one { 0 } else { ORTHONORMAL });
        out
    }

    /// `transformPosition`, by `Vector3f.mulPosition`'s paths.
    pub fn transform_position(&self, [x, y, z]: Vec3) -> Vec3 {
        let m = &self.m;
        if self.properties & IDENTITY != 0 {
            return [x, y, z];
        }
        if self.properties & TRANSLATION != 0 {
            return [x + m[3][0], y + m[3][1], z + m[3][2]];
        }
        std::array::from_fn(|row| {
            fma(
                m[0][row],
                x,
                fma(m[1][row], y, fma(m[2][row], z, m[3][row])),
            )
        })
    }

    /// `transformDirection` (`Vector3f.mulDirection`, which has no shortcut paths).
    pub fn transform_direction(&self, [x, y, z]: Vec3) -> Vec3 {
        let m = &self.m;
        std::array::from_fn(|row| fma(m[0][row], x, fma(m[1][row], y, m[2][row] * z)))
    }

    /// `invertAffine`.
    pub fn invert_affine(&self) -> Mat4 {
        let m = &self.m;
        let (m00, m01, m02) = (m[0][0], m[0][1], m[0][2]);
        let (m10, m11, m12) = (m[1][0], m[1][1], m[1][2]);
        let (m20, m21, m22) = (m[2][0], m[2][1], m[2][2]);
        let (m30, m31, m32) = (m[3][0], m[3][1], m[3][2]);
        let m11m00 = m00 * m11;
        let m10m01 = m01 * m10;
        let m10m02 = m02 * m10;
        let m12m00 = m00 * m12;
        let m12m01 = m01 * m12;
        let m11m02 = m02 * m11;
        let det = (m11m00 - m10m01) * m22 + (m10m02 - m12m00) * m21 + (m12m01 - m11m02) * m20;
        let s = 1.0 / det;
        let m10m22 = m10 * m22;
        let m10m21 = m10 * m21;
        let m11m22 = m11 * m22;
        let m11m20 = m11 * m20;
        let m12m21 = m12 * m21;
        let m12m20 = m12 * m20;
        let m20m02 = m20 * m02;
        let m20m01 = m20 * m01;
        let m21m02 = m21 * m02;
        let m21m00 = m21 * m00;
        let m22m01 = m22 * m01;
        let m22m00 = m22 * m00;
        let nm31 = (m20m02 * m31 - m20m01 * m32 + m21m00 * m32 - m21m02 * m30 + m22m01 * m30
            - m22m00 * m31)
            * s;
        let nm32 = (m11m02 * m30 - m12m01 * m30 + m12m00 * m31 - m10m02 * m31 + m10m01 * m32
            - m11m00 * m32)
            * s;
        let nm30 = (m10m22 * m31 - m10m21 * m32 + m11m20 * m32 - m11m22 * m30 + m12m21 * m30
            - m12m20 * m31)
            * s;
        Mat4 {
            m: [
                [
                    (m11m22 - m12m21) * s,
                    (m21m02 - m22m01) * s,
                    (m12m01 - m11m02) * s,
                    0.0,
                ],
                [
                    (m12m20 - m10m22) * s,
                    (m22m00 - m20m02) * s,
                    (m10m02 - m12m00) * s,
                    0.0,
                ],
                [
                    (m10m21 - m11m20) * s,
                    (m20m01 - m21m00) * s,
                    (m11m00 - m10m01) * s,
                    0.0,
                ],
                [nm30, nm31, nm32, 1.0],
            ],
            properties: AFFINE,
        }
    }

    pub fn is_finite(&self) -> bool {
        self.m.iter().flatten().all(|v| v.is_finite())
    }
}

/// A column-major 3×3 matrix, `m[col][row]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub m: [[f32; 3]; 3],
}

impl Mat3 {
    /// `new Matrix3f().scaling(x, y, z)`.
    pub fn scaling([x, y, z]: Vec3) -> Mat3 {
        Mat3 {
            m: [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, z]],
        }
    }

    /// `this.mul(right)`.
    pub fn mul(&self, right: &Mat3) -> Mat3 {
        let (a, b) = (&self.m, &right.m);
        Mat3 {
            m: std::array::from_fn(|col| {
                std::array::from_fn(|row| {
                    fma(
                        a[0][row],
                        b[col][0],
                        fma(a[1][row], b[col][1], a[2][row] * b[col][2]),
                    )
                })
            }),
        }
    }
}

/// A quaternion (x, y, z, w).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub fn rotate_x(&self, angle: f32) -> Quat {
        let s = sin(angle * 0.5);
        let c = cos_from_sin(s, angle * 0.5);
        Quat {
            x: self.w * s + self.x * c,
            y: self.y * c + self.z * s,
            z: self.z * c - self.y * s,
            w: self.w * c - self.x * s,
        }
    }

    pub fn rotate_y(&self, angle: f32) -> Quat {
        let s = sin(angle * 0.5);
        let c = cos_from_sin(s, angle * 0.5);
        Quat {
            x: self.x * c - self.z * s,
            y: self.w * s + self.y * c,
            z: self.x * s + self.z * c,
            w: self.w * c - self.y * s,
        }
    }

    /// The 3×3 rotation JOML builds from the quaternion, by column.
    fn rotation_terms(&self) -> [[f32; 3]; 3] {
        let Quat { x, y, z, w } = *self;
        let (w2, x2, y2, z2) = (w * w, x * x, y * y, z * z);
        let zw = z * w;
        let dzw = zw + zw;
        let xy = x * y;
        let dxy = xy + xy;
        let xz = x * z;
        let dxz = xz + xz;
        let yw = y * w;
        let dyw = yw + yw;
        let yz = y * z;
        let dyz = yz + yz;
        let xw = x * w;
        let dxw = xw + xw;
        [
            [w2 + x2 - z2 - y2, dxy + dzw, dxz - dyw],
            [-dzw + dxy, y2 - z2 + w2 - x2, dyz + dxw],
            [dyw + dxz, dyz - dxw, z2 - y2 - x2 + w2],
        ]
    }
}

/// `GeometryUtils.normal`: the normalised cross product of (v1 − v0) and (v2 − v0).
pub fn normal(v0: Vec3, v1: Vec3, v2: Vec3) -> Vec3 {
    let n = [
        ((v1[1] - v0[1]) * (v2[2] - v0[2])) - ((v1[2] - v0[2]) * (v2[1] - v0[1])),
        ((v1[2] - v0[2]) * (v2[0] - v0[0])) - ((v1[0] - v0[0]) * (v2[2] - v0[2])),
        ((v1[0] - v0[0]) * (v2[1] - v0[1])) - ((v1[1] - v0[1]) * (v2[0] - v0[0])),
    ];
    let scalar = invsqrt(fma(n[0], n[0], fma(n[1], n[1], n[2] * n[2])));
    n.map(|c| c * scalar)
}

/// `Vector3f.dot`.
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    fma(a[0], b[0], fma(a[1], b[1], a[2] * b[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_turn_about_y_takes_x_to_minus_z() {
        let q = Quat::IDENTITY.rotate_y((std::f64::consts::PI / 2.0) as f32);
        let v = Mat4::identity()
            .rotate(&q)
            .transform_direction([1.0, 0.0, 0.0]);
        assert!(
            v[0].abs() < 1e-6 && v[1].abs() < 1e-6 && (v[2] + 1.0).abs() < 1e-6,
            "{v:?}"
        );
    }

    #[test]
    fn invert_affine_undoes_a_rotation() {
        let m = Mat4::rotation_axis(0.3, [0.0, 1.0, 0.0]);
        let back = m.invert_affine().mul(&m);
        for (col, column) in back.m.iter().take(3).enumerate() {
            for (row, value) in column.iter().take(3).enumerate() {
                let want = if col == row { 1.0 } else { 0.0 };
                assert!((value - want).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn cos_from_sin_takes_the_quadrant_s_sign() {
        assert!(cos_from_sin(sin(2.0), 2.0) < 0.0);
        assert!(cos_from_sin(sin(-0.5), -0.5) > 0.0);
    }
}
