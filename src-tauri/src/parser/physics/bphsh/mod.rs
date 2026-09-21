//! TOTK Phive mesh shapes (`Phive/Shape/Dcc/*.bphsh`): the `hknpMeshShape`
//! collision meshes static actors reference through a ShapeParam
//! `PhshMesh` entry.
//!
//! - [`reader`] / [`writer`] parse and serialize the file byte for byte.
//! - [`builder`] rebuilds a shape from a triangle soup the way Havok's
//!   `hknpMeshShape` builder does (a port of PhiveConverter's
//!   reimplementation, adjusted to what TOTK's SDK 2022 build emits).
//! - [`obj`] exchanges the geometry with Wavefront OBJ plus a material JSON.
//! - [`materials`] names TOTK's material ids and user shape tags.

pub mod builder;
pub mod materials;
pub mod obj;
pub mod reader;
pub mod shape;
#[cfg(test)]
mod tests;
pub mod writer;
