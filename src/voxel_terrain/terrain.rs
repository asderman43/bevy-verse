
use bevy_panorbit_camera::PanOrbitCamera;

use bevy::{
    asset::RenderAssetUsages,
    math::{vec3, I16Vec3, U16Vec2, U16Vec3},
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
    state::commands,
    utils::{hashbrown::HashMap, HashSet},
};
use libnoise::prelude::*;

use crate::voxel_terrain::{noise::get_noise, util::XYZ};

use super::{table::*, voxel::{self, *}};

pub fn update_mesh(mut terrain: ResMut<Terrain>, mut meshes: ResMut<Assets<Mesh>>) {
    // if terrain.changed {
    //     let mesh = meshes.get_mut(terrain.mesh_handle.as_ref().unwrap().id());

    //     if let Some(_mesh) = mesh {
    //         *_mesh = terrain.mesh();
    //         terrain.changed = false;
    //     } else {
    //     }
    // }
}

#[derive(Resource)]
pub struct Terrain {
    chunks: HashMap<IVec3, VoxelMap>,
    // TODO: Ez legyen queue
    remesh: HashSet<IVec3>,
    size: usize,
    real_size: usize,
    // TODO: ez legyen egy enum hogy kell-e vagy sem interpolation.
    isolevel: i8,
}
enum ChunkMesh {
    NoHandle(Mesh)
}
impl Terrain {
    pub fn get_size(&self) -> usize {
        self.size
    }
    //TODO: Ez lehet hogy auto hozzon letre a hashmapban cuccot.
    pub fn generate_chunk(&mut self, id: IVec3) {
        let mut vm = VoxelMap::new(self.size);
        let sub_position = -id.signum();
        for position in XYZ::new(self.real_size) {
            let global_pos = self.local_to_global(position, id) + sub_position;
            
            let value = get_noise(global_pos);

            vm.set(position, self.real_size, value);
        }
        self.chunks.insert(id, vm);
        self.remesh.insert(id);
        
    }
    fn mesh_chunk(&self, voxel_map: &VoxelMap) -> Mesh {
        voxel_map.create_mesh(
            self.size,
            self.real_size,
            self.isolevel,
        )
        
    }
    /// Gets the local position in the chunk, and current chunk. Outputs the position of the Voxel in the world.
    fn local_to_global(&self, position: (usize, usize, usize), chunk_id: IVec3) -> IVec3 {
        IVec3::new(position.0 as i32, position.1 as i32, position.2 as i32)
            + chunk_id * self.real_size as i32
    }
    /// Gets the global position and outputs the position inside the chunk, and the id of the chunk.
    fn global_to_local(&self, position: IVec3) -> (IVec3, (usize, usize, usize)) {
        let loc_pos = position % self.real_size as i32;
        (
            position / self.real_size as i32,
            (loc_pos.x as usize, loc_pos.y as usize, loc_pos.z as usize),
        )
    }
    
    pub fn new(size: usize, isolevel: i8) -> Self {
        
        Terrain {
            chunks: HashMap::new(),
            remesh: HashSet::with_capacity(5),
            size,
            real_size: size + 1,
            isolevel,
        }
        //terrain.generate_chunk(IVec3::ZERO);

        //terrain.chunks.insert(IVec3::ZERO, chunk);

        //terrain
    }
    pub fn get(&self, position: IVec3) -> Option<i8> {
        let (chunk_id, get_pos) = self.global_to_local(position);

        let chunk = self.chunks.get(&chunk_id);
        if let Some(voxel) = chunk {
            Some(voxel.get(get_pos, self.real_size))
        } else {
            None
        }
    }
    pub fn set(&mut self, position: IVec3, value: i8) -> Result<(), ()> {
        let (chunk_id, set_pos) = self.global_to_local(position);
        let chunk = self.chunks.get_mut(&chunk_id);
        if let Some(voxel) = chunk {
            voxel.set(set_pos, self.real_size, value);
            self.remesh.insert(chunk_id);
            Ok(())
        } else {
            Err(())
        }
    }
    pub fn update(&mut self, position: &IVec3) {
        match self.chunks.get(position) {
            Some(mut chunk) => todo!("Update chunk"),
            None => todo!("Generate chunk"),
        }
    }

    pub fn create_mesh(&mut self, meshes: &mut ResMut<Assets<Mesh>>, commands: &mut Commands, materials: &mut ResMut<Assets<StandardMaterial>>) {
        for chunk_id in self.remesh.iter() {
            let mesh = {
                let voxel_map = self.chunks.get(chunk_id).unwrap();
                self.mesh_chunk(&voxel_map)
            };
            
            let voxel_map = self.chunks.get_mut(chunk_id).unwrap();
            match &voxel_map.mesh_handle {
                Some(handle_id) => {
                    let _mesh = meshes.get_mut(handle_id).unwrap();
                    *_mesh = mesh;
                },
                None => {
                    let handle = meshes.add(mesh);
                    let positon = {
                        let scaled = (self.size as i32) * chunk_id;
                        vec3(scaled.x as f32, scaled.y as f32, scaled.z as f32)
                    };
                    commands.spawn((
                            Mesh3d(handle.clone()),
                            MeshMaterial3d(materials.add(StandardMaterial {
                                ..default()
                            })),
                            Transform::from_translation(positon)
                        ));

                        voxel_map.mesh_handle = Some(handle);
                }
            }
        }
        //let mesh = self.mesh();

        //self.mesh_handle = Some(meshes.add(mesh));
    }
    // HARDCODED!!!!
}


#[cfg(test)]
mod tests {
    use bevy::math::IVec3;

    use super::Terrain;

    #[test]
    fn get_pos() {
        let mut terrain = Terrain::new(2, 3);
        terrain.generate_chunk(IVec3::ZERO);
        terrain.generate_chunk(IVec3::X);
        
        let chunk_0 = terrain.chunks.get(&IVec3::ZERO).unwrap();
        let chunk_1 = terrain.chunks.get(&IVec3::X).unwrap();

        println!("{:?} {:?}", terrain.local_to_global((2, 0, 0), IVec3::ZERO), terrain.local_to_global((0, 0, 0), IVec3::X));


        //println!("{:?} {:?}", terrain.get(IVec3::ZERO), chunk_0.get(terrain.global_to_local(IVec3::ZERO).1, terrain.real_size));
    }
}