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
    chunk: Vec<i8>,
    pub mesh_handle: Option<Handle<Mesh>>,
}
impl VoxelMap {
    pub fn new(size: usize) -> VoxelMap {
        let real_size = size + 1;
        VoxelMap {
            chunk: vec![0; real_size.pow(3)],
            mesh_handle: None,
        }
    }
    pub fn get(&self, position: (usize, usize, usize), real_size: usize) -> i8 {
        self.chunk[position.0 + position.1 * real_size + position.2 * real_size * real_size]
    }
    pub fn set(&mut self, position: (usize, usize, usize), real_size: usize, value: i8) {
        self.chunk[position.0 + position.1 * real_size + position.2 * real_size * real_size] =
            value;
    }
    pub fn create_mesh(&self, size: usize, real_size: usize, isolevel: i8) -> Mesh {
        let mut vertex_buffer = vec![];

        for (x, y, z) in XYZ::new(size) {
            // TODO: Ezt megnezni miert nem mukodott alapbol.
            let mut value: usize = 0;
            let corners = [
                self.get((x, y, z), real_size),
                self.get((x + 1, y, z), real_size),
                self.get((x + 1, y + 1, z), real_size),
                self.get((x, y + 1, z), real_size),
                self.get((x, y, z + 1), real_size),
                self.get((x + 1, y, z + 1), real_size),
                self.get((x + 1, y + 1, z + 1), real_size),
                self.get((x, y + 1, z + 1), real_size),
            ];

            // let corners = [
            //     self.get((x, y, z), real_size),
            //     self.get((x + 1, y, z), real_size),
            //     self.get((x, y + 1, z), real_size),
            //     self.get((x + 1, y + 1, z), real_size),
            //     self.get((x, y, z + 1), real_size),
            //     self.get((x + 1, y, z + 1), real_size),
            //     self.get((x, y + 1, z + 1), real_size),
            //     self.get((x + 1, y + 1, z + 1), real_size),
            // ];

            for i in 0..8 {
                if corners[i] <= isolevel {
                    value |= 1 << i;
                }
            }
            let triangle = TRIANGLE_TABLE[value];

            let mut index = 0;

            loop {
                let edge = triangle[index];
                if edge == -1 {
                    break;
                }

                let vertices = EDGE_VERTEX_INDICES[edge as usize];

                let vert = vec3(x as f32, y as f32, z as f32)
                    + interpolation(
                        (VERTEX_POSITIONS[vertices.0], VERTEX_POSITIONS[vertices.1]),
                        (corners[REMAP[vertices.0]], corners[REMAP[vertices.1]]),
                        isolevel,
                    );

                //    + (VERTEX_POSITIONS[vertices.0] + VERTEX_POSITIONS[vertices.1]) / 2.0;
                vertex_buffer.push(vert);

                index += 1;
            }
        }
        let indices_buffer = (0..vertex_buffer.len()).map(|i| i as u16).collect();

        vertex_buffer = vertex_buffer
            .iter()
            .map(|vert| vec3(vert.x, vert.y, vert.z))
            .collect();

        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertex_buffer)
        .with_inserted_indices(Indices::U16(indices_buffer))
        .with_duplicated_vertices()
        .with_computed_flat_normals()
    }
}
fn interpolation(vertices: (Vec3, Vec3), values: (i8, i8), isolevel: i8) -> Vec3 {
    vertices.0
        + (isolevel as f32 - values.0 as f32) * (vertices.1 - vertices.0)
            / (values.1 as f32 - values.0 as f32)
}
