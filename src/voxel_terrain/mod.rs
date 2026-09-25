use bevy::prelude::*;

pub mod debug;
pub mod isosurface;
pub mod terrain;
pub mod voxel;
pub mod voxels;

// Re-export utils so submodules can use `super::util`.
mod utils {
    pub mod util;
}
pub use utils::util;

pub struct VoxelTerrainPlugin;

impl Plugin for VoxelTerrainPlugin {
    fn build(&self, _app: &mut App) {
        // app.add_plugins(VoxelTerrainDebugPlugin)
        //     .insert_resource(Terrain::new(8));
    }
}
