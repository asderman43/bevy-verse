use std::fmt;

use super::{
    table::{EDGE_VERTEX_INDICES, REMAP, TRIANGLE_TABLE, VERTEX_POSITIONS}, terrain::Terrain, util::XYZ, voxel::VoxelMap
};
use bevy::{
    asset::RenderAssetUsages,
    math::vec3,
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};
pub struct IsosurfaceExtractor {
    isolevel: i8,
}
pub struct GridBuffer {
    pub grid: Vec<i8>,
    size: usize,
}
impl fmt::Display for GridBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "{:?}", self.grid)
    }
}
impl GridBuffer {
    pub fn new(
        chunk: &VoxelMap,
        terrain: &Terrain
    ) -> Self {
        let size = terrain.get_size();
        // TODO: ezt most kikapcsoltam!
        let real_size = size;

        // Haromszor atgondoltam es ez kell mert ha parallelizalok akkor majd jol jon a
        // cacheleshez, vagy hat a row by row.
        // Ha kornyezo chunk nincs akkor ott az ilyen border szeruseg 0akkal lesz megtoltve.

        let mut vec = Vec::with_capacity(real_size.pow(3));
        for position in XYZ::new(real_size) {
            let (x,y,z) = position;
            // ki kell vonogatni valahogy
                    // let glubal = terrain.local_to_global((x,y,z), chunk.id);
                    // println!("{:?}", terrain.global_to_local(glubal));
                    let val = if let Some(val) = terrain.get(terrain.local_to_global((x,y,z), chunk.id)){
                        val
                    }
                    else {
                        0
                    };
                    
                    vec.push(val);
        }
        
        GridBuffer { grid: vec, size: (real_size) }
    }
    fn get(&self, position: (usize, usize, usize)) -> i8 {
        self.grid[position.0 + position.1 * self.size + position.2 * self.size * self.size]
    }
}

impl IsosurfaceExtractor {
    pub fn new(isolevel: i8) -> Self {
        IsosurfaceExtractor { isolevel }
    }
    pub fn create_mesh(&self, grid: GridBuffer) -> Mesh {
        let mut vertex_buffer = vec![];

        for (x, y, z) in XYZ::new(grid.size-1) {
            // TODO: Ezt megnezni miert nem mukodott alapbol.
            let mut value: usize = 0;
            let corners = [
                grid.get((x, y, z)),
                grid.get((x + 1, y, z)),
                grid.get((x + 1, y + 1, z)),
                grid.get((x, y + 1, z)),
                grid.get((x, y, z + 1)),
                grid.get((x + 1, y, z + 1)),
                grid.get((x + 1, y + 1, z + 1)),
                grid.get((x, y + 1, z + 1)),
            ];

            // let corners = [
            //     grid.get((x, y, z)),
            //     grid.get((x + 1, y, z)),
            //     grid.get((x, y + 1, z)),
            //     grid.get((x + 1, y + 1, z)),
            //     grid.get((x, y, z + 1)),
            //     grid.get((x + 1, y, z + 1)),
            //     grid.get((x, y + 1, z + 1)),
            //     grid.get((x + 1, y + 1, z + 1)),
            // ];

            for i in 0..8 {
                if corners[i] <= self.isolevel {
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
                    // + interpolation(
                    //     (VERTEX_POSITIONS[vertices.0], VERTEX_POSITIONS[vertices.1]),
                    //     (corners[REMAP[vertices.0]], corners[REMAP[vertices.1]]),
                    //     self.isolevel,
                    // );

                   + (VERTEX_POSITIONS[vertices.0] + VERTEX_POSITIONS[vertices.1]) / 2.0;
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
