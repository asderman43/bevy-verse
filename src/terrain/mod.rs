pub mod debug;
pub mod table;
pub mod voxel;
pub mod util;


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
pub fn update_mesh(mut terrain: ResMut<Terrain>, mut meshes: ResMut<Assets<Mesh>>) {
    if terrain.changed {
        let mesh = meshes.get_mut(terrain.mesh_handle.as_ref().unwrap().id());

        if let Some(_mesh) = mesh{
            
            *_mesh = terrain.mesh();
            terrain.changed = false;
        }
        else {
           
        }
        
    }
}

#[derive(Resource)]
pub struct Terrain {
    chunks: HashMap<IVec3, Voxel>,
    changed: bool,
    size: usize,
    real_size: usize,
    pub mesh_handle: Option<Handle<Mesh>>,
}

impl Terrain {
    pub fn new(size: usize) -> Self {
        let chunk = Voxel::new(size);

        Terrain {
            chunks: HashMap::from([(IVec3::ZERO, chunk)]),
            changed: true,
            size,
            real_size: size + 1,
            mesh_handle: None,
        }
    }
    pub fn get(&self, position: IVec3) -> Option<i8> {
        let chunk_id = position / self.real_size as i32;
        let loc_position = position % self.real_size as i32;
        let chunk = self.chunks.get(&chunk_id);
        if let Some(voxel) = chunk {
            let get_pos = (
                loc_position.x as usize,
                loc_position.y as usize,
                loc_position.z as usize,
            );

            Some(voxel.get(get_pos, self.real_size))
        } else {
            None
        }
    }
    pub fn set(&mut self, position: IVec3, value: i8) -> Result<(), ()> {
        let chunk_id = position / self.real_size as i32;
        let loc_position = position % self.real_size as i32;
        let chunk = self.chunks.get_mut(&chunk_id);
        if let Some(voxel) = chunk {
            let set_pos = (
                loc_position.x as usize,
                loc_position.y as usize,
                loc_position.z as usize,
            );
            voxel.set(set_pos, self.real_size, value);
            self.changed = true;
            Ok(())
        } else {
            Err(())
        }
    }
    pub fn create_mesh(&mut self, meshes: &mut ResMut<Assets<Mesh>>) {
        let mesh = self.mesh();

        self.mesh_handle = Some(meshes.add(mesh));
    }
    /// HARDCODED!!!!
    fn mesh(&self) -> Mesh {
        self.chunks
            .get(&IVec3::ZERO)
            .unwrap()
            .create_mesh(self.size, self.real_size)
    }
    
}






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
