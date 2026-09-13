//! Bit-exact ports of the OpenTK 3 math (`Toolbox/Lib/OpenTK.dll`) that
//! Switch Toolbox runs over imported vertices and bones: `Matrix4`
//! (row-major, row vectors), `Vector3` and `Quaternion`. Every operation
//! keeps the association order and precision of the IL so the results match
//! Toolbox's output bit for bit; nothing here is meant to be pretty math.

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// `OpenTK.Matrix4`: `m[row][column]`, translation in row 3.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f32; 4]; 4],
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UNIT_X: Vec3 = Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UNIT_Y: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const UNIT_Z: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    pub fn from_array(v: [f32; 3]) -> Vec3 {
        Vec3::new(v[0], v[1], v[2])
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    /// `X * X + Y * Y + Z * Z` in float.
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// `(float)Math.Sqrt(LengthSquared)`.
    pub fn length(self) -> f32 {
        f64::from(self.length_squared()).sqrt() as f32
    }

    /// `Vector3.Normalize`: scale by `1 / Length`.
    pub fn normalized(self) -> Vec3 {
        let scale = 1.0f32 / self.length();
        Vec3::new(self.x * scale, self.y * scale, self.z * scale)
    }

    pub fn dot(a: Vec3, b: Vec3) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    }

    pub fn add(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
    }

    pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
    }

    pub fn scale(self, scale: f32) -> Vec3 {
        Vec3::new(self.x * scale, self.y * scale, self.z * scale)
    }

    /// `Vector3.TransformPosition(pos, mat)`.
    pub fn transform_position(self, mat: &Mat4) -> Vec3 {
        let m = &mat.m;
        Vec3::new(
            self.x * m[0][0] + self.y * m[1][0] + self.z * m[2][0] + m[3][0],
            self.x * m[0][1] + self.y * m[1][1] + self.z * m[2][1] + m[3][1],
            self.x * m[0][2] + self.y * m[1][2] + self.z * m[2][2] + m[3][2],
        )
    }

    /// `Vector3.TransformNormalInverse(norm, invMat)`.
    pub fn transform_normal_inverse(self, inverse: &Mat4) -> Vec3 {
        let m = &inverse.m;
        Vec3::new(
            self.x * m[0][0] + self.y * m[0][1] + self.z * m[0][2],
            self.x * m[1][0] + self.y * m[1][1] + self.z * m[1][2],
            self.x * m[2][0] + self.y * m[2][1] + self.z * m[2][2],
        )
    }

    /// `Vector3.TransformNormal(norm, mat)`: inverts the matrix first.
    pub fn transform_normal(self, mat: &Mat4) -> Result<Vec3, String> {
        Ok(self.transform_normal_inverse(&mat.inverted()?))
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Quat {
        Quat { x, y, z, w }
    }

    pub fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    pub fn length_squared(self) -> f32 {
        self.w * self.w + self.xyz().length_squared()
    }

    pub fn length(self) -> f32 {
        f64::from(self.length_squared()).sqrt() as f32
    }

    /// `Quaternion.Normalize`.
    pub fn normalized(self) -> Quat {
        let scale = 1.0f32 / self.length();
        let xyz = self.xyz().scale(scale);
        Quat::new(xyz.x, xyz.y, xyz.z, self.w * scale)
    }

    /// `Quaternion.FromAxisAngle`.
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        if axis.length_squared() == 0.0 {
            return Quat::IDENTITY;
        }
        let angle = angle * 0.5f32;
        let axis = axis.normalized();
        let xyz = axis.scale(f64::from(angle).sin() as f32);
        let w = f64::from(angle).cos() as f32;
        Quat::new(xyz.x, xyz.y, xyz.z, w).normalized()
    }

    /// `Quaternion.Multiply(left, right)`.
    pub fn multiply(left: Quat, right: Quat) -> Quat {
        let xyz = Vec3::add(
            Vec3::add(left.xyz().scale(right.w), right.xyz().scale(left.w)),
            Vec3::cross(left.xyz(), right.xyz()),
        );
        let w = left.w * right.w - Vec3::dot(left.xyz(), right.xyz());
        Quat::new(xyz.x, xyz.y, xyz.z, w)
    }

    /// `q * -1`.
    pub fn negated(self) -> Quat {
        Quat::new(self.x * -1.0, self.y * -1.0, self.z * -1.0, self.w * -1.0)
    }

    /// `Quaternion.ToAxisAngle()`: (axis, angle).
    pub fn to_axis_angle(self) -> (Vec3, f32) {
        let q = if self.w.abs() > 1.0 {
            self.normalized()
        } else {
            self
        };
        let angle = 2.0f32 * (f64::from(q.w).acos() as f32);
        let den = (1.0f64 - f64::from(q.w * q.w)).sqrt() as f32;
        let axis = if den > 0.0001f32 {
            Vec3::new(q.x / den, q.y / den, q.z / den)
        } else {
            Vec3::UNIT_X
        };
        (axis, angle)
    }
}

/// `STMath.FromEulerAngles(Vector3)`: `Z * Y * X` axis-angle quaternions,
/// flipped to a positive `W`.
pub fn quat_from_euler(rotation: Vec3) -> Quat {
    let x = Quat::from_axis_angle(Vec3::UNIT_X, rotation.x);
    let y = Quat::from_axis_angle(Vec3::UNIT_Y, rotation.y);
    let z = Quat::from_axis_angle(Vec3::UNIT_Z, rotation.z);
    let q = Quat::multiply(Quat::multiply(z, y), x);
    if q.w < 0.0 {
        q.negated()
    } else {
        q
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// `AssimpHelper.TKMatrix`: the transposed Assimp matrix.
    pub fn from_assimp(a: &[[f32; 4]; 4]) -> Mat4 {
        let mut m = [[0.0f32; 4]; 4];
        for (row, out) in m.iter_mut().enumerate() {
            for (column, cell) in out.iter_mut().enumerate() {
                *cell = a[column][row];
            }
        }
        Mat4 { m }
    }

    pub fn create_scale(scale: Vec3) -> Mat4 {
        let mut out = Mat4::IDENTITY;
        out.m[0][0] = scale.x;
        out.m[1][1] = scale.y;
        out.m[2][2] = scale.z;
        out
    }

    pub fn create_translation(v: Vec3) -> Mat4 {
        let mut out = Mat4::IDENTITY;
        out.m[3][0] = v.x;
        out.m[3][1] = v.y;
        out.m[3][2] = v.z;
        out
    }

    /// `Matrix4.CreateFromAxisAngle`.
    pub fn create_from_axis_angle(axis: Vec3, angle: f32) -> Mat4 {
        let axis = axis.normalized();
        let (x, y, z) = (axis.x, axis.y, axis.z);
        let cos = f64::from(-angle).cos() as f32;
        let sin = f64::from(-angle).sin() as f32;
        let t = 1.0f32 - cos;
        let t_xx = t * x * x;
        let t_xy = t * x * y;
        let t_xz = t * x * z;
        let t_yy = t * y * y;
        let t_yz = t * y * z;
        let t_zz = t * z * z;
        let sin_x = sin * x;
        let sin_y = sin * y;
        let sin_z = sin * z;
        Mat4 {
            m: [
                [t_xx + cos, t_xy - sin_z, t_xz + sin_y, 0.0],
                [t_xy + sin_z, t_yy + cos, t_yz - sin_x, 0.0],
                [t_xz - sin_y, t_yz + sin_x, t_zz + cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// `Matrix4.CreateFromQuaternion`: axis/angle in this OpenTK version.
    pub fn create_from_quaternion(q: Quat) -> Mat4 {
        let (axis, angle) = q.to_axis_angle();
        Mat4::create_from_axis_angle(axis, angle)
    }

    /// `Matrix4.Mult(left, right)`: `left * right` with row vectors.
    pub fn mult(left: &Mat4, right: &Mat4) -> Mat4 {
        let (l, r) = (&left.m, &right.m);
        let mut m = [[0.0f32; 4]; 4];
        for (row, out) in m.iter_mut().enumerate() {
            for (column, cell) in out.iter_mut().enumerate() {
                *cell = l[row][0] * r[0][column]
                    + l[row][1] * r[1][column]
                    + l[row][2] * r[2][column]
                    + l[row][3] * r[3][column];
            }
        }
        Mat4 { m }
    }

    /// `Matrix4.Invert`: the adjugate formulation of this OpenTK version.
    pub fn inverted(&self) -> Result<Mat4, String> {
        let m: [f32; 16] = [
            self.m[0][0],
            self.m[0][1],
            self.m[0][2],
            self.m[0][3],
            self.m[1][0],
            self.m[1][1],
            self.m[1][2],
            self.m[1][3],
            self.m[2][0],
            self.m[2][1],
            self.m[2][2],
            self.m[2][3],
            self.m[3][0],
            self.m[3][1],
            self.m[3][2],
            self.m[3][3],
        ];
        let mut inv = [0.0f32; 16];
        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14]
            + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];
        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14]
            - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];
        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13]
            + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];
        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13]
            - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];
        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14]
            - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];
        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14]
            + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];
        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13]
            - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];
        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13]
            + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];
        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14]
            + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];
        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14]
            - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];
        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13]
            + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];
        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13]
            - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];
        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10]
            - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];
        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10]
            + m[8] * m[2] * m[7]
            - m[8] * m[3] * m[6];
        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9]
            - m[8] * m[1] * m[7]
            + m[8] * m[3] * m[5];
        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9]
            + m[8] * m[1] * m[6]
            - m[8] * m[2] * m[5];
        let mut det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det == 0.0 {
            return Err("Matrix is singular and cannot be inverted.".to_owned());
        }
        det = 1.0f32 / det;
        let mut out = [[0.0f32; 4]; 4];
        for (index, value) in inv.iter().enumerate() {
            out[index / 4][index % 4] = value * det;
        }
        Ok(Mat4 { m: out })
    }

    /// `Matrix4.ExtractRotation(row_normalise: true)`.
    pub fn extract_rotation(&self) -> Quat {
        let row = |i: usize| Vec3::new(self.m[i][0], self.m[i][1], self.m[i][2]).normalized();
        let (row0, row1, row2) = (row(0), row(1), row(2));
        let (x, y, z, w): (f32, f32, f32, f32);
        let trace = 0.25 * (f64::from(row0.x + row1.y + row2.z) + 1.0);
        if trace > 0.0 {
            let mut sq = trace.sqrt();
            w = sq as f32;
            sq = 1.0 / (4.0 * sq);
            x = (f64::from(row1.z - row2.y) * sq) as f32;
            y = (f64::from(row2.x - row0.z) * sq) as f32;
            z = (f64::from(row0.y - row1.x) * sq) as f32;
        } else if row0.x > row1.y && row0.x > row2.z {
            let mut sq =
                2.0 * (1.0 + f64::from(row0.x) - f64::from(row1.y) - f64::from(row2.z)).sqrt();
            x = (0.25 * sq) as f32;
            sq = 1.0 / sq;
            w = (f64::from(row2.y - row1.z) * sq) as f32;
            y = (f64::from(row1.x + row0.y) * sq) as f32;
            z = (f64::from(row2.x + row0.z) * sq) as f32;
        } else if row1.y > row2.z {
            let mut sq =
                2.0 * (1.0 + f64::from(row1.y) - f64::from(row0.x) - f64::from(row2.z)).sqrt();
            y = (0.25 * sq) as f32;
            sq = 1.0 / sq;
            w = (f64::from(row2.x - row0.z) * sq) as f32;
            x = (f64::from(row1.x + row0.y) * sq) as f32;
            z = (f64::from(row2.y + row1.z) * sq) as f32;
        } else {
            let mut sq =
                2.0 * (1.0 + f64::from(row2.z) - f64::from(row0.x) - f64::from(row1.y)).sqrt();
            z = (0.25 * sq) as f32;
            sq = 1.0 / sq;
            w = (f64::from(row1.x - row0.y) * sq) as f32;
            x = (f64::from(row2.x + row0.z) * sq) as f32;
            y = (f64::from(row2.y + row1.z) * sq) as f32;
        }
        Quat::new(x, y, z, w).normalized()
    }
}
