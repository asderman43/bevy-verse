use std::collections::HashMap;
pub mod picking;

use bevy::prelude::*;

use crate::voxel_terrain::{voxels::debug::init};

pub struct VoxelTerrainDebugPlugin;

struct Debug {
    handles: HashMap<u16, Handle<GizmoAsset>>,
}

impl Plugin for VoxelTerrainDebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init);
            
    }
}
