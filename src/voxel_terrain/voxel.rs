use bevy::{
    asset::RenderAssetUsages,
    math::vec3,
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};

use super::{
    table::*,
    util::{position_from_index, XYZ},
};

pub struct VoxelMap {
    pub id: IVec3,
    pub chunk: Vec<i8>,
    pub mesh_handle: Option<Handle<Mesh>>,
}
impl VoxelMap {
    pub fn new(size: usize, id: IVec3) -> VoxelMap {
        VoxelMap {
            id,
            chunk: vec![0; size.pow(3)],
            mesh_handle: None,
        }
    }
    pub fn get(&self, position: (usize, usize, usize), size: usize) -> i8 {
        self.chunk[position.0 + position.1 * size + position.2 * size * size]
    }
    pub fn set(&mut self, position: (usize, usize, usize), size: usize, value: i8) {
        self.chunk[position.0 + position.1 * size + position.2 * size * size] =
            value;
    }
    /// THIS IS A DEBUG FUNCTION
    pub fn set_chunk(&mut self, vec: Vec<i8>) {
        self.chunk = vec;
    }
    pub fn get_chunk(&self) -> Vec<i8> {
        self.chunk.clone()
    }
}
