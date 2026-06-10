use bevy::prelude::*;

use crate::voxel_terrain::{debug::VoxelTerrainDebugPlugin, terrain::Terrain};

pub mod debug;
pub mod march;
pub mod terrain;
pub mod voxels;
pub mod voxel;

// Re-export utils so march.rs can use `super::table` and `super::util`
mod utils {
    pub mod table;
    pub mod util;
}
pub use utils::table;
pub use utils::util;

pub struct VoxelTerrainPlugin;

impl Plugin for VoxelTerrainPlugin {
    fn build(&self, app: &mut App) {
        // app.add_plugins(VoxelTerrainDebugPlugin)
        //     .insert_resource(Terrain::new(8));
    }
}

pub struct MarchingCubesPlugin;

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (march::setup, march::march_debug));
    }
}
