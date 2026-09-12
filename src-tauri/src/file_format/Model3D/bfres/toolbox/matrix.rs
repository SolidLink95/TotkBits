//! Bit-exact port of the `System.Numerics` float math Toolbox uses in
//! `MatrixExenstion.CalculateInverseMatrix` when it recomputes the inverse
//! bind matrices of a skeleton on save (`FSKL.CalculateIndices`).
//! Every operation keeps the association order of the .NET implementation.

use super::model::Bone;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4 {
    pub m11: f32,
    pub m12: f32,
    pub m13: f32,
    pub m14: f32,
    pub m21: f32,
    pub m22: f32,
    pub m23: f32,
    pub m24: f32,
    pub m31: f32,
    pub m32: f32,
    pub m33: f32,
    pub m34: f32,
    pub m41: f32,
    pub m42: f32,
    pub m43: f32,
    pub m44: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Matrix4 {
    pub const IDENTITY: Matrix4 = Matrix4 {
        m11: 1.0,
        m12: 0.0,
        m13: 0.0,
        m14: 0.0,
        m21: 0.0,
        m22: 1.0,
        m23: 0.0,
        m24: 0.0,
        m31: 0.0,
        m32: 0.0,
        m33: 1.0,
        m34: 0.0,
        m41: 0.0,
        m42: 0.0,
        m43: 0.0,
        m44: 1.0,
    };

    pub fn create_translation(x: f32, y: f32, z: f32) -> Matrix4 {
        let mut m = Matrix4::IDENTITY;
        m.m41 = x;
        m.m42 = y;
        m.m43 = z;
        m
    }

    pub fn create_from_quaternion(q: Quat) -> Matrix4 {
        let xx = q.x * q.x;
        let yy = q.y * q.y;
        let zz = q.z * q.z;
        let xy = q.x * q.y;
        let wz = q.z * q.w;
        let xz = q.z * q.x;
        let wy = q.y * q.w;
        let yz = q.y * q.z;
        let wx = q.x * q.w;
        Matrix4 {
            m11: 1.0 - 2.0 * (yy + zz),
            m12: 2.0 * (xy + wz),
            m13: 2.0 * (xz - wy),
            m14: 0.0,
            m21: 2.0 * (xy - wz),
            m22: 1.0 - 2.0 * (zz + xx),
            m23: 2.0 * (yz + wx),
            m24: 0.0,
            m31: 2.0 * (xz + wy),
            m32: 2.0 * (yz - wx),
            m33: 1.0 - 2.0 * (yy + xx),
            m34: 0.0,
            m41: 0.0,
            m42: 0.0,
            m43: 0.0,
            m44: 1.0,
        }
    }

    /// `Matrix4x4.Multiply(value1, value2)` (row-vector convention).
    pub fn multiply(a: &Matrix4, b: &Matrix4) -> Matrix4 {
        Matrix4 {
            m11: a.m11 * b.m11 + a.m12 * b.m21 + a.m13 * b.m31 + a.m14 * b.m41,
            m12: a.m11 * b.m12 + a.m12 * b.m22 + a.m13 * b.m32 + a.m14 * b.m42,
            m13: a.m11 * b.m13 + a.m12 * b.m23 + a.m13 * b.m33 + a.m14 * b.m43,
            m14: a.m11 * b.m14 + a.m12 * b.m24 + a.m13 * b.m34 + a.m14 * b.m44,
            m21: a.m21 * b.m11 + a.m22 * b.m21 + a.m23 * b.m31 + a.m24 * b.m41,
            m22: a.m21 * b.m12 + a.m22 * b.m22 + a.m23 * b.m32 + a.m24 * b.m42,
            m23: a.m21 * b.m13 + a.m22 * b.m23 + a.m23 * b.m33 + a.m24 * b.m43,
            m24: a.m21 * b.m14 + a.m22 * b.m24 + a.m23 * b.m34 + a.m24 * b.m44,
            m31: a.m31 * b.m11 + a.m32 * b.m21 + a.m33 * b.m31 + a.m34 * b.m41,
            m32: a.m31 * b.m12 + a.m32 * b.m22 + a.m33 * b.m32 + a.m34 * b.m42,
            m33: a.m31 * b.m13 + a.m32 * b.m23 + a.m33 * b.m33 + a.m34 * b.m43,
            m34: a.m31 * b.m14 + a.m32 * b.m24 + a.m33 * b.m34 + a.m34 * b.m44,
            m41: a.m41 * b.m11 + a.m42 * b.m21 + a.m43 * b.m31 + a.m44 * b.m41,
            m42: a.m41 * b.m12 + a.m42 * b.m22 + a.m43 * b.m32 + a.m44 * b.m42,
            m43: a.m41 * b.m13 + a.m42 * b.m23 + a.m43 * b.m33 + a.m44 * b.m43,
            m44: a.m41 * b.m14 + a.m42 * b.m24 + a.m43 * b.m34 + a.m44 * b.m44,
        }
    }

    /// `Matrix4x4.Invert`; returns the NaN matrix when singular, like .NET.
    pub fn invert(&self) -> Matrix4 {
        let (a, b, c, d) = (self.m11, self.m12, self.m13, self.m14);
        let (e, f, g, h) = (self.m21, self.m22, self.m23, self.m24);
        let (i, j, k, l) = (self.m31, self.m32, self.m33, self.m34);
        let (m, n, o, p) = (self.m41, self.m42, self.m43, self.m44);

        let kp_lo = k * p - l * o;
        let jp_ln = j * p - l * n;
        let jo_kn = j * o - k * n;
        let ip_lm = i * p - l * m;
        let io_km = i * o - k * m;
        let in_jm = i * n - j * m;

        let a11 = f * kp_lo - g * jp_ln + h * jo_kn;
        let a12 = -(e * kp_lo - g * ip_lm + h * io_km);
        let a13 = e * jp_ln - f * ip_lm + h * in_jm;
        let a14 = -(e * jo_kn - f * io_km + g * in_jm);

        let det = a * a11 + b * a12 + c * a13 + d * a14;
        if det.abs() < f32::from_bits(1) {
            let nan = f32::NAN;
            return Matrix4 {
                m11: nan,
                m12: nan,
                m13: nan,
                m14: nan,
                m21: nan,
                m22: nan,
                m23: nan,
                m24: nan,
                m31: nan,
                m32: nan,
                m33: nan,
                m34: nan,
                m41: nan,
                m42: nan,
                m43: nan,
                m44: nan,
            };
        }
        let inv_det = 1.0f32 / det;

        let mut r = Matrix4::IDENTITY;
        r.m11 = a11 * inv_det;
        r.m21 = a12 * inv_det;
        r.m31 = a13 * inv_det;
        r.m41 = a14 * inv_det;

        r.m12 = -(b * kp_lo - c * jp_ln + d * jo_kn) * inv_det;
        r.m22 = (a * kp_lo - c * ip_lm + d * io_km) * inv_det;
        r.m32 = -(a * jp_ln - b * ip_lm + d * in_jm) * inv_det;
        r.m42 = (a * jo_kn - b * io_km + c * in_jm) * inv_det;

        let gp_ho = g * p - h * o;
        let fp_hn = f * p - h * n;
        let fo_gn = f * o - g * n;
        let ep_hm = e * p - h * m;
        let eo_gm = e * o - g * m;
        let en_fm = e * n - f * m;

        r.m13 = (b * gp_ho - c * fp_hn + d * fo_gn) * inv_det;
        r.m23 = -(a * gp_ho - c * ep_hm + d * eo_gm) * inv_det;
        r.m33 = (a * fp_hn - b * ep_hm + d * en_fm) * inv_det;
        r.m43 = -(a * fo_gn - b * eo_gm + c * en_fm) * inv_det;

        let gl_hk = g * l - h * k;
        let fl_hj = f * l - h * j;
        let fk_gj = f * k - g * j;
        let el_hi = e * l - h * i;
        let ek_gi = e * k - g * i;
        let ej_fi = e * j - f * i;

        r.m14 = -(b * gl_hk - c * fl_hj + d * fk_gj) * inv_det;
        r.m24 = (a * gl_hk - c * el_hi + d * ek_gi) * inv_det;
        r.m34 = -(a * fl_hj - b * el_hi + d * ej_fi) * inv_det;
        r.m44 = (a * fk_gj - b * ek_gi + c * ej_fi) * inv_det;
        r
    }
}

impl Quat {
    pub fn create_from_axis_angle(axis: [f32; 3], angle: f32) -> Quat {
        let half = angle * 0.5f32;
        let s = (half as f64).sin() as f32;
        let c = (half as f64).cos() as f32;
        Quat {
            x: axis[0] * s,
            y: axis[1] * s,
            z: axis[2] * s,
            w: c,
        }
    }

    /// `Quaternion.Multiply(value1, value2)`.
    pub fn multiply(q1: Quat, q2: Quat) -> Quat {
        let (q1x, q1y, q1z, q1w) = (q1.x, q1.y, q1.z, q1.w);
        let (q2x, q2y, q2z, q2w) = (q2.x, q2.y, q2.z, q2.w);
        let cx = q1y * q2z - q1z * q2y;
        let cy = q1z * q2x - q1x * q2z;
        let cz = q1x * q2y - q1y * q2x;
        let dot = q1x * q2x + q1y * q2y + q1z * q2z;
        Quat {
            x: q1x * q2w + q2x * q1w + cx,
            y: q1y * q2w + q2y * q1w + cy,
            z: q1z * q2w + q2z * q1w + cz,
            w: q1w * q2w - dot,
        }
    }

    pub fn scale(self, factor: f32) -> Quat {
        Quat {
            x: self.x * factor,
            y: self.y * factor,
            z: self.z * factor,
            w: self.w * factor,
        }
    }
}

fn quat_from_quat(x: f32, y: f32, z: f32, w: f32) -> Quat {
    let mut q = Quat { x, y, z, w };
    if q.w < 0.0 {
        q = q.scale(-1.0);
    }
    q
}

pub(crate) fn quat_from_euler(x: f32, y: f32, z: f32) -> Quat {
    let x_rotation = Quat::create_from_axis_angle([1.0, 0.0, 0.0], x);
    let y_rotation = Quat::create_from_axis_angle([0.0, 1.0, 0.0], y);
    let z_rotation = Quat::create_from_axis_angle([0.0, 0.0, 1.0], z);
    let mut q = Quat::multiply(Quat::multiply(z_rotation, y_rotation), x_rotation);
    if q.w < 0.0 {
        q = q.scale(-1.0);
    }
    q
}

/// `MatrixExenstion.CalculateTransformMatrix`: rotation then translation,
/// scale is computed but never applied by Toolbox.
fn transform_matrix(bone: &Bone) -> Matrix4 {
    let trans = Matrix4::create_translation(bone.position[0], bone.position[1], bone.position[2]);
    let quat = if bone.uses_euler() {
        // STBone stores the file's XYZ vector as its Euler rotation.
        Matrix4::create_from_quaternion(quat_from_euler(
            bone.rotation[0],
            bone.rotation[1],
            bone.rotation[2],
        ))
    } else {
        Matrix4::create_from_quaternion(quat_from_quat(
            bone.rotation[0],
            bone.rotation[1],
            bone.rotation[2],
            bone.rotation[3],
        ))
    };
    Matrix4::multiply(&quat, &trans)
}

/// `MatrixExenstion.CalculateInverseMatrix(bone).transform` (recursive).
pub(crate) fn world_transform(bones: &[Bone], index: usize) -> Matrix4 {
    let bone = &bones[index];
    let mut transform = Matrix4::IDENTITY;
    if bone.parent_index >= 0 && (bone.parent_index as usize) < bones.len() {
        let parent = world_transform(bones, bone.parent_index as usize);
        transform = Matrix4::multiply(&transform, &parent);
    }
    Matrix4::multiply(&transform_matrix(bone), &transform)
}

fn normalize_negative_zero(value: f32) -> f32 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

/// `MatrixExenstion.GetMatrixInverted(bone)` as the 12 floats Syroot writes
/// for a `Matrix3x4` (row major, M11..M14, M21..M24, M31..M34).
pub fn inverse_bind_matrix(bones: &[Bone], index: usize) -> [f32; 12] {
    let world = world_transform(bones, index);
    let mut inv = world.invert();
    inv.m11 = normalize_negative_zero(inv.m11);
    inv.m12 = normalize_negative_zero(inv.m12);
    inv.m13 = normalize_negative_zero(inv.m13);
    inv.m14 = normalize_negative_zero(inv.m14);
    inv.m21 = normalize_negative_zero(inv.m21);
    inv.m22 = normalize_negative_zero(inv.m22);
    inv.m23 = normalize_negative_zero(inv.m23);
    inv.m24 = normalize_negative_zero(inv.m24);
    inv.m31 = normalize_negative_zero(inv.m31);
    inv.m32 = normalize_negative_zero(inv.m32);
    inv.m33 = normalize_negative_zero(inv.m33);
    inv.m34 = normalize_negative_zero(inv.m34);
    [
        inv.m11, inv.m21, inv.m31, inv.m41, //
        inv.m12, inv.m22, inv.m32, inv.m42, //
        inv.m13, inv.m23, inv.m33, inv.m43,
    ]
}
