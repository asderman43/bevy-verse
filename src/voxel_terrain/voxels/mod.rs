use bevy::prelude::*;

use crate::voxel_terrain::voxels::grid::grid::Grid;

pub mod debug;
pub mod grid;
pub mod octree;

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        todo!()
    }
}
