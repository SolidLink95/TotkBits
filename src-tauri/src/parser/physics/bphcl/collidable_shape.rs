use super::Vector4;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "shape")]
pub enum CollidableShape {
    Capsule {
        start: Vector4,
        end: Vector4,
        radius: f32,
    },
    Sphere {
        center: Vector4,
        radius: f32,
    },
    TaperedCapsule {
        start: Vector4,
        end: Vector4,
        start_radius: f32,
        end_radius: f32,
    },
    Plane {
        equation: Vector4,
    },
    Unknown {
        class_name: String,
        kind: u32,
    },
}
