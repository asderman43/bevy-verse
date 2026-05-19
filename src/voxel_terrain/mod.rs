use bevy::prelude::*;

use crate::voxel_terrain::{debug::VoxelTerrainDebugPlugin, terrain::Terrain};

// pub mod march;
// pub mod table;
// pub mod terrain;
// pub mod util;
// pub mod voxel;
pub mod debug;
pub mod terrain;
pub mod voxels;

pub struct VoxelTerrainPlugin;

impl Plugin for VoxelTerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VoxelTerrainDebugPlugin)
            .insert_resource(Terrain::new(8));
    }
}

/*
use bevy_panorbit_camera::PanOrbitCamera;
use table::*;
use voxel::*;

use bevy::{
    asset::RenderAssetUsages,
    math::{vec3, I16Vec3, U16Vec2, U16Vec3},
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
    state::commands,

};

#[cfg(test)]
mod tests {
    use bevy::math::IVec3;

    use super::{march::GridBuffer, terrain::Terrain, util::XYZ};

    #[test]
    fn try_iter() {
        for a in XYZ::new(2) {
            println!("{:?}", a)
        }
    }

    #[test]
    fn grid_buffer() {
        let mut terrain = Terrain::new(2, 2);

        terrain.generate_chunk(IVec3::ZERO);
        {
            let chunk_0 = terrain.chunks.get_mut(&IVec3::ZERO).unwrap();
            chunk_0.set_chunk(vec![0, 1, 0, 1, 0, 1, 0, 1]);
        }
        let chunk_0 = terrain.chunks.get(&IVec3::ZERO).unwrap();

        let global_pos = terrain.local_to_global((3, 3, 3), IVec3::ZERO);
        let local_pos = terrain.global_to_local(global_pos);
        println!("{}", terrain.get_size());
        // println!("{}", global_pos);
        // println!("{} {:?}", local_pos.0, local_pos.1);
        let buffer = GridBuffer::new(chunk_0, &terrain);

        assert!(
            buffer.grid
                == vec![
                    0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ]
        )
    }
}
*/
