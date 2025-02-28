pub mod debug;
pub mod table;
pub mod terrain;
pub mod util;
pub mod voxel;

pub mod noise;

use bevy_panorbit_camera::PanOrbitCamera;
use table::*;
use voxel::*;

use bevy::{
    asset::RenderAssetUsages,
    math::{vec3, I16Vec3, U16Vec2, U16Vec3},
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
    state::commands,
    utils::hashbrown::HashMap,
};

#[cfg(test)]
mod tests {
    use super::util::XYZ;

    #[test]
    fn try_iter() {
        for a in XYZ::new(2) {
            println!("{:?}", a)
        }
    }
}
