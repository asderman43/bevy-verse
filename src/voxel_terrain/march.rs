use std::{fmt, ptr};

use crate::voxel_terrain::table::{EDGE_INTERSECTION, TRIANGLE_COUNT};

use super::{
    table::{EDGE_VERTEX_INDICES, ORDER, TRIANGLE_TABLE, VERTEX_POSITIONS}, util::XYZ,
};
use bevy::{
    asset::RenderAssetUsages, color::palettes::css::{BLUE, GREEN, RED}, math::vec3, prelude::*, render::mesh::{Indices, PrimitiveTopology}
};

fn test_test_test() -> (Vec<i8>, i8, usize) {
    test_map_4()
}

pub fn setup(mut commands: Commands,
    _asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,) {
        let (map, isolevel, size) = test_test_test();
        
        let gd = GridBuffer { grid: map, size };

        let fe = IsosurfaceExtractor::new(isolevel);

        let pass_1_results = fe.pass_1(&gd);
        // println!("{:?}", pass_1_results.0);

        let metadata = IsosurfaceExtractor::pass_2(size, &pass_1_results.0, pass_1_results.1);
        
        let metadata = IsosurfaceExtractor::pass_3(metadata, size);
        for m in &metadata.0 {
            println!("{:?}", m);
        }
        
        let output = IsosurfaceExtractor::pass_4(size, metadata.0, metadata.1, metadata.2, &pass_1_results.0);
        
        println!("\nIndices:\n{:?}", output.0);
        println!("Vertices:\n{:?}", output.1);

        // let cube_mesh_handle: Handle<Mesh> = meshes.add(IsosurfaceExtractor::final_output(output.0, output.1));
        let cube_mesh_handle = meshes.add(fe.marching_cubes(gd));
        // Render the mesh with the custom texture, and add the marker.
        commands.spawn((
            Mesh3d(cube_mesh_handle),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.0, 0.0),
                cull_mode: None,
                ..default()
            })),
            
        ));
    }

/// Builds the 12 wireframe edges of an axis-aligned box spanning
/// from the origin to (size-1) on each axis.
fn create_box(size: usize) -> Vec<(Vec3, Vec3)> {
    let n = (size.saturating_sub(1)) as f32;
    let corners = [
        vec3(0.0, 0.0, 0.0),
        vec3(n, 0.0, 0.0),
        vec3(n, n, 0.0),
        vec3(0.0, n, 0.0),
        vec3(0.0, 0.0, n),
        vec3(n, 0.0, n),
        vec3(n, n, n),
        vec3(0.0, n, n),
    ];
    let edges = [
        (0, 1), (1, 2), (2, 3), (3, 0), // bottom face
        (4, 5), (5, 6), (6, 7), (7, 4), // top face
        (0, 4), (1, 5), (2, 6), (3, 7), // vertical edges
    ];
    edges
        .iter()
        .map(|&(a, b)| (corners[a], corners[b]))
        .collect()
}

pub fn march_debug(mut gizmo_assets: ResMut<Assets<GizmoAsset>>, mut commands: Commands) {
    let (map, isolevel, size) = test_test_test();
    let lines = create_box(size);

    let mut gizmo = GizmoAsset::new();
    for line in lines {
        gizmo.line(line.0, line.1, RED);
    }
    
    gizmo.arrow(
        Vec3::ZERO - Vec3::new(0.1, 0.1, 0.1),
        Vec3::Y - Vec3::new(0.1, 0.1, 0.1),
        GREEN,
    );
    gizmo.arrow(
        Vec3::ZERO - Vec3::new(0.1, 0.1, 0.1),
        Vec3::X - Vec3::new(0.1, 0.1, 0.1),
        RED,
    );
    gizmo.arrow(
        Vec3::ZERO - Vec3::new(0.1, 0.1, 0.1),
        Vec3::Z - Vec3::new(0.1, 0.1, 0.1),
        BLUE,
    );
    // dbg!(size);
    let mut index = 0;
    for (x, y, z) in XYZ::new(size) {
        let value = map[x + y * size + z * size * size];
        
                let color = if value <= isolevel {
                    RED
                } else {
                    GREEN
                };
            // dbg!(((x,y,z), color));
            gizmo.sphere(vec3(x as f32, y as f32, z as f32), 0.1, color);
            index += 1;
    }
    // dbg!(index);
    commands.spawn((
        Gizmo {
            handle: gizmo_assets.add(gizmo),
            line_config: GizmoLineConfig {
                width: 2.,
                ..default()
            },
            ..default()
        },
        Transform::IDENTITY,
    ));
}
pub struct IsosurfaceExtractor {
    isolevel: i8,
}
pub struct GridBuffer {
    pub grid: Vec<i8>,
    pub size: usize,
}
impl fmt::Display for GridBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "{:?}", self.grid)
    }
}
impl GridBuffer {
    fn get(&self, position: (usize, usize, usize)) -> i8 {
        self.grid[position.0 + position.1 * self.size + position.2 * self.size * self.size]
    }
    fn get_edge_index(&self, position: (usize, usize)) -> usize {
        self.size * position.0 + self.size * self.size * position.1
    }
}
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Metadata {
    yz: (usize, usize),
    left_trim: usize,
    right_trim: usize,
    x_intersects: u32,
    y_intersects: u32,
    z_intersects: u32,
    tris_count: u32,
}
#[derive(Default, Clone, Debug)]
pub struct EdgeID {
    yz: (usize, usize),
    x: u32,
    y: u32,
    z: u32,
    indices_id: u32,
    left_trim: usize,
    right_trim: usize,
}
impl IsosurfaceExtractor {
    pub fn new(isolevel: i8) -> Self {
        IsosurfaceExtractor { isolevel }
    }
    pub fn create_mesh(&self, grid: GridBuffer) -> Mesh {
        let size = grid.size;

        let pass_1_results = self.pass_1(&grid);
        let pass_2_results = IsosurfaceExtractor::pass_2(size, &pass_1_results.0, pass_1_results.1);
        let pass_3_results = IsosurfaceExtractor::pass_3(pass_2_results, size);

        let output = IsosurfaceExtractor::pass_4(size, pass_3_results.0, pass_3_results.1, pass_3_results.2, &pass_1_results.0);
    

        Self::final_output(output.0, output.1)
    }
    pub fn marching_cubes(&self, grid: GridBuffer) -> Mesh {
        let mut vertex_buffer = vec![];

        for (x, y, z) in XYZ::new(grid.size - 1) {
            // TODO: Ezt megnezni miert nem mukodott alapbol.
            let mut value: usize = 0;

            let corners = [
                grid.get((x, y, z)),
                grid.get((x + 1, y, z)),
                grid.get((x, y + 1, z)),
                grid.get((x + 1, y + 1, z)),
                grid.get((x, y, z + 1)),
                grid.get((x + 1, y, z + 1)),
                grid.get((x, y + 1, z + 1)),
                grid.get((x + 1, y + 1, z + 1)),
            ];

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
                    + interpolation(
                        (VERTEX_POSITIONS[vertices.0], VERTEX_POSITIONS[vertices.1]),
                        (corners[vertices.0], corners[vertices.1]),
                        self.isolevel,
                    );

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
    fn pass_1(&self, grid_buffer: &GridBuffer) -> (Vec<u8>, Vec<Metadata>) {
        let size = grid_buffer.size;
        let buffer_length = size - 1;
        let mut edge_buffer = Vec::with_capacity(buffer_length * size.pow(2));
        let mut edge_data = Vec::with_capacity(size.pow(2));

        unsafe {
            edge_buffer.set_len(buffer_length * size.pow(2));
            edge_data.set_len(size.pow(2));
        }

        for y in 0..size {
            for z in 0..size {
                let edge_jk = (y, z);
                let start_index = grid_buffer.get_edge_index(edge_jk);

                let buffer_index = y * buffer_length + z * buffer_length * size;

                let mut increment = 0;
                let mut xl = 0;
                let mut xr = 0;

                let mut x_intersects = 0;

                while increment < buffer_length {
                    let index = increment + start_index;
                    if increment >= size - 1 {
                        break;
                    }
                    let mut edge_case: u8 = 0;
                    let x_edge = (grid_buffer.grid[index], grid_buffer.grid[index + 1]);

                    if x_edge.0 <= self.isolevel {
                        edge_case |= 1 << 0;
                    }

                    if x_edge.1 <= self.isolevel {
                        edge_case |= 1 << 1;
                    }

                    if edge_case > 0 && edge_case != 3 {
                        if xl == 0 {
                            xl = increment;
                        }
                        if edge_case != 3 {
                            x_intersects += 1;
                        }
                        xr = increment;
                    }

                    edge_buffer[buffer_index + increment] = edge_case;
                    edge_data[y + z * size] = Metadata {
                        left_trim: xl,
                        right_trim: xr,
                        x_intersects,
                        y_intersects: 0,
                        z_intersects: 0,
                        tris_count: 0,
                        yz: (0, 0),
                    };
                    increment += 1;
                }
            }
        }

        (edge_buffer, edge_data)
    }
    // TODO: ez ujra kell irni.
    fn pass_2(size: usize, edge_buffer: &Vec<u8>, mut edge_data: Vec<Metadata>) -> Vec<Metadata> {
        let max_index = size - 2;
        let buffer_length = size - 1;
        let x_max = buffer_length - 1;

        for y in 0..=max_index {
            for z in 0..=max_index {
                let edge_data = unsafe {
                    [
                        ptr::addr_of_mut!(edge_data[y + z * size]).as_mut().unwrap(),
                        ptr::addr_of_mut!(edge_data[y + (z + 1) * size]).as_mut().unwrap(),
                        ptr::addr_of_mut!(edge_data[(y + 1) + z * size]).as_mut().unwrap(),
                        ptr::addr_of_mut!(edge_data[(y + 1) + (z + 1) * size]).as_mut().unwrap(),
                    ]
                };

                // Adjusted trim values.
                let (left_trim, right_trim, _x_intersects) = {
                    let (left_trim, right_trim) = {
                        let mut left_trim = edge_data[0].left_trim;
                        let mut right_trim = edge_data[0].right_trim;
                        for data in &edge_data {
                            if data.left_trim < left_trim {
                                left_trim = data.left_trim;
                            }
                            if data.right_trim > right_trim {
                                right_trim = data.right_trim;
                            }
                        }
                        (left_trim, right_trim)
                    };
                    let x_intersects = edge_data[0].x_intersects;
                    (left_trim, right_trim, x_intersects)
                };
                let mut y_intersects: u32 = 0;
                let mut z_intersects = 0;
                let mut tris_count: u32 = 0;
                for x in left_trim..=right_trim {
                    let mut value = 0;

                    let edges = {
                        [
                            edge_buffer[y * buffer_length + z * buffer_length * size + x],
                            edge_buffer[(y + 1) * buffer_length + z * buffer_length * size + x],
                            edge_buffer[y * buffer_length + (z + 1) * buffer_length * size + x],
                            edge_buffer[(y + 1) * buffer_length + (z + 1) * buffer_length * size + x],
                        ]
                    };

                    for i in 0..4 {
                        value |= edges[i] << (i * 2);
                    }

                    let edge_intersections = EDGE_INTERSECTION[value as usize];

                    tris_count += TRIANGLE_COUNT[value as usize] as u32;

                    y_intersects += ((edge_intersections >> 4) & 1) as u32;
                    z_intersects += ((edge_intersections >> 8) & 1) as u32;
                    
                    if x == x_max {
                        y_intersects += ((edge_intersections >> 5) & 1) as u32;
                        z_intersects += ((edge_intersections >> 9) & 1) as u32;
                    }
                    if y == max_index {
                        edge_data[2].z_intersects += ((edge_intersections >> 10) & 1) as u32;
                    }

                    if z == max_index {
                        edge_data[1].y_intersects += ((edge_intersections >> 6) & 1) as u32;
                    }

                    if x == x_max && y == max_index {
                        edge_data[2].z_intersects += ((edge_intersections >> 11) & 1) as u32;
                    }

                    if x == x_max && z == max_index {
                        edge_data[1].y_intersects += ((edge_intersections >> 7) & 1) as u32;
                    }
                }
                // Last X
                edge_data[0].tris_count = tris_count;
                edge_data[0].y_intersects = y_intersects;
                edge_data[0].z_intersects = z_intersects;
                edge_data[0].yz = (y, z);
                if z == max_index {
                    edge_data[1].yz = (y, z+1);
                }
                if y == max_index {
                    edge_data[2].yz = (y+1, z);
                }
                if z == max_index && y == max_index {
                    edge_data[3].yz = (y+z, z+1); 
                }
            }
        }
        edge_data
    }
    fn pass_3(metadata: Vec<Metadata>, size: usize) -> (Vec<EdgeID>, Vec<u32>, Vec<Vec3>) {
        let mut triangle_count = 0;
        let mut edge_ids: Vec<EdgeID> = Vec::with_capacity(size.pow(2));
        // X>Y>Z
        let mut overall = 0;
        for metadata in metadata {
            let indices_id = triangle_count * 3;
            triangle_count += metadata.tris_count;
            let edge = EdgeID {
                x: overall,
                y: overall + metadata.x_intersects,
                z: overall + metadata.x_intersects + metadata.y_intersects,
                left_trim: metadata.left_trim,
                right_trim: metadata.right_trim,
                indices_id,
                yz: metadata.yz,
            };
            overall = overall + metadata.x_intersects + metadata.y_intersects + metadata.z_intersects;
            edge_ids.push(edge);
        }
        let indices_len = (triangle_count * 3) as usize;
        let mut indices: Vec<u32> = Vec::with_capacity(indices_len);
        unsafe {
            indices.set_len(indices_len);
        }
        let vertices_len = (overall) as usize;
        let mut vertices: Vec<Vec3> = Vec::with_capacity(vertices_len);
        unsafe {
            vertices.set_len(vertices_len);
        }
        (edge_ids, indices, vertices)
    }
    fn pass_4(
        size: usize,
        edge_ids: Vec<EdgeID>,
        mut indices_buffer: Vec<u32>,
        mut vertices_buffer: Vec<Vec3>,
        edge_buffer: &Vec<u8>,
    ) -> (Vec<u32>, Vec<Vec3>) {
        let real_size = size - 1;
        let buffer_length = size - 1;
        for y in 0..real_size {
            for z in 0..real_size {
                let edge_data = [
                    &edge_ids[y + z * size],
                    &edge_ids[(y) + (z + 1) * size],
                    &edge_ids[(y + 1) + (z) * size],
                    &edge_ids[(y + 1) + (z + 1) * size],
                ];
                let (left_trim, right_trim) = {
                    let mut left_trim = edge_data[0].left_trim;
                    let mut right_trim = edge_data[0].right_trim;
                    for data in edge_data {
                        if data.left_trim < left_trim {
                            left_trim = data.left_trim;
                        }
                        if data.right_trim > right_trim {
                            right_trim = data.right_trim;
                        }
                    }
                    (left_trim, right_trim)
                };
                let mut ids = [
                    /*  0 */ edge_data[0].x,
                    /*  1 */ edge_data[1].x,
                    /*  2 */ edge_data[2].x,
                    /*  3 */ edge_data[3].x,
                    /*  4 */ edge_data[0].y,
                    /*  5 */ edge_data[0].y,
                    /*  6 */ edge_data[1].y,
                    /*  7 */ edge_data[1].y,
                    /*  8 */ edge_data[0].z,
                    /*  9 */ edge_data[0].z,
                    /* 10 */ edge_data[2].z,
                    /* 11 */ edge_data[2].z,
                ];
                let mut indices_id = edge_data[0].indices_id;
                for x in left_trim..=right_trim {
                    let edges = [
                        edge_buffer[y * buffer_length + z * buffer_length * size + x],
                        edge_buffer[(y + 1) * buffer_length + z * buffer_length * size + x],
                        edge_buffer[y * buffer_length + (z + 1) * buffer_length * size + x],
                        edge_buffer[(y + 1) * buffer_length + (z + 1) * buffer_length * size + x],
                    ];
                    let mut value: u8 = 0;
                    for i in 0..4 {
                        value |= edges[i] << (i * 2);
                    }
                    let value = value as usize;
                    let intersections = EDGE_INTERSECTION[value]; 
                    if x == left_trim {
                        ids = [
                            /*  0 */ ids[0],
                            /*  1 */ ids[1],
                            /*  2 */ ids[2],
                            /*  3 */ ids[3],
                            /*  4 */ ids[4],
                            /*  5 */ ids[4] + ((intersections >> 4) & 1) as u32,
                            /*  6 */ ids[6],
                            /*  7 */ ids[6] + ((intersections >> 6) & 1) as u32,
                            /*  8 */ ids[8],
                            /*  9 */ ids[8] + ((intersections >> 8) & 1) as u32,
                            /* 10 */ ids[10],
                            /* 11 */ ids[10] + ((intersections >> 10) & 1) as u32,
                        ];
                    }
                    else {
                        ids = [
                            /*  0 */ ids[0] + ((intersections >> 0) & 1) as u32,
                            /*  1 */ ids[1] + ((intersections >> 1) & 1) as u32,
                            /*  2 */ ids[2] + ((intersections >> 2) & 1) as u32,
                            /*  3 */ ids[3] + ((intersections >> 3) & 1) as u32,
                            /*  4 */ ids[4],
                            /*  5 */ ids[4] + ((intersections >> 4) & 1) as u32,
                            /*  6 */ ids[6],
                            /*  7 */ ids[6] + ((intersections >> 6) & 1) as u32,
                            /*  8 */ ids[8],
                            /*  9 */ ids[8] + ((intersections >> 8) & 1) as u32,
                            /* 10 */ ids[10],
                            /* 11 */ ids[10] + ((intersections >> 10) & 1) as u32,
                        ];
                    }
                    
                    let marching_case = TRIANGLE_TABLE[value];
                    let mut index = 0;
                    
                    loop {
                        let edge = marching_case[index];
                        if edge == -1 { break; }
                        
                        let vertices = EDGE_VERTEX_INDICES[edge as usize];

                        let vert = vec3(x as f32, y as f32, z as f32)
                               + (VERTEX_POSITIONS[vertices.0] + VERTEX_POSITIONS[vertices.1]) / 2.0;
                        let vert_id = ids[ORDER[edge as usize]];
                        vertices_buffer[vert_id as usize] = vert;

                        indices_buffer[indices_id as usize + index] = ids[ORDER[edge as usize]];
                        index += 1;
                    }
                    let triangle_count = TRIANGLE_COUNT[value] as u32;
                    indices_id += triangle_count * 3;

                    ids = [
                        /*  0 */ ids[0],
                        /*  1 */ ids[1],
                        /*  2 */ ids[2],
                        /*  3 */ ids[3],
                        /*  4 */ ids[5],
                        /*  5 */ ids[5],
                        /*  6 */ ids[7],
                        /*  7 */ ids[7],
                        /*  8 */ ids[9],
                        /*  9 */ ids[9],
                        /* 10 */ ids[11],
                        /* 11 */ ids[11],
                    ];
                    
                }
            }
        }
        (indices_buffer, vertices_buffer)
    }
    fn final_output(indices_buffer: Vec<u32>, vertices_buffer: Vec<Vec3>) -> Mesh{
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices_buffer)
        .with_inserted_indices(Indices::U32(indices_buffer))
        .with_computed_normals()
    }
}

fn interpolation(vertices: (Vec3, Vec3), values: (i8, i8), isolevel: i8) -> Vec3 {
    vertices.0
        + (isolevel as f32 - values.0 as f32) * (vertices.1 - vertices.0)
            / (values.1 as f32 - values.0 as f32)
}

pub fn test_map() -> (Vec<i8>, i8, usize) {
        (
            vec![
                5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 3, 5,
                5, 5, 2, 3, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5,
                5, 2, 0, 2, 2, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
                5, 2, 1, 4, 5, 5, 5, 5, 2, 0, 0, 0, 3, 3, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5,
                5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 1, 5, 5, 5, 5, 5, 3, 0, 0, 0, 5, 5, 5, 5, 0, 0, 0, 0,
                1, 5, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 1, 4,
                5, 5, 5, 5, 5, 0, 0, 0, 3, 5, 5, 5, 0, 0, 0, 0, 0, 5, 5, 5, 0, 0, 0, 0, 0, 1, 5, 1,
                0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
                5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 3, 0, 5, 5, 5, 5, 0, 0, 0, 0, 5, 5, 5,
                5, 0, 0, 0, 0, 0, 5, 5, 0, 0, 0, 0, 0, 0, 5, 5, 0, 0, 0, 0, 5, 5, 5, 5, 5, 5, 5, 5,
                5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 5, 5, 5, 5, 5, 5, 5, 0, 5, 5, 5,
                5, 5, 5, 3, 0, 2, 5, 5, 5, 5, 0, 0, 0, 0, 5, 5, 5, 0, 0, 0, 0, 0, 4, 5, 4, 0, 0, 0,
                5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 3, 5, 5, 5,
                5, 5, 5, 5, 0, 5, 5, 5, 5, 5, 5, 5, 0, 3, 5, 5, 5, 5, 5, 5, 0, 0, 5, 5, 5, 5, 5, 0,
                0, 0, 4, 5, 5, 5, 1, 0,
            ],
            2,
            8,
        )
    }
    pub fn test_map_2(size: usize) -> (Vec<i8>, i8, usize) {
        let mut map = vec![0; size.pow(3)];
        for y in 1..size - 1 {
            map[size * y] = 4;
        }
        (map, 2, size)
    }
    pub fn test_map_3() -> (Vec<i8>, i8, usize) {
        let mut map = vec![0; 27];
        let size = 3;
        map[1 + size * 1 + size * size * 1] = 5;
        
        (map, 2, 3)
    }
    pub fn test_map_4() -> (Vec<i8>, i8, usize) {
        const SIZE: usize = 3;
        let mut map = vec![0; SIZE.pow(3)];
        fn coord_fn(xyz: (usize, usize, usize)) -> usize {
            xyz.0 + xyz.1*SIZE + xyz.2*SIZE*SIZE
        }
        let set_coords = [
            (2,1,0),
            (2,1,2),

            (0, 0, 0),
            (1, 0, 0),
            (2, 0, 0),
            (0, 0, 1),
            (0, 0, 2),
        ];
        for set_coord in set_coords {
            map[coord_fn(set_coord)] = 5;
        }
        (map, 2, SIZE)
    }
#[cfg(test)]
pub mod tests {
    use bevy::math::{vec3, Vec3};

    use crate::voxel_terrain::{march::{test_map, test_map_3, Metadata}, table::{EDGE_INTERSECTION, TRIANGLE_COUNT, TRIANGLE_TABLE}};

    use super::{GridBuffer, IsosurfaceExtractor};

    
    #[test]
    fn vec_test() {
        let mut vec = Vec::<Vec3>::with_capacity(10);

        unsafe {
            vec.set_len(10);
        }
        vec[5] = vec3(1., 1., 1.);
        println!("{:?}", vec);
    }
    #[test]
    fn pass_1() {
        let (map, isolevel, size) = test_map();

        let gd = GridBuffer { grid: map, size };

        let fe = IsosurfaceExtractor::new(isolevel);
        let pass_1_results = fe.pass_1(&gd);
        let (y, z) = (3, 2);
        let start: usize = y * (gd.size - 1) + z * (gd.size - 1) * gd.size;
        for i in 0..gd.size - 1 {
            print!("{} ", pass_1_results.0[start + i]);
        }
        println!("{:?}", pass_1_results.1[y + z * gd.size])
    }

    #[test]
    fn pass_2() {
        let (map, isolevel, size) = test_map();

        let gd = GridBuffer { grid: map, size };

        let fe = IsosurfaceExtractor::new(isolevel);

        let pass_1_results = fe.pass_1(&gd);

        let metadata = IsosurfaceExtractor::pass_2(size, &pass_1_results.0, pass_1_results.1);
        println!("{:#?}", metadata);
    }
    #[test]
    fn pass_3() {
        let (map, isolevel, size) = test_map_3();

        let gd = GridBuffer { grid: map, size };

        let fe = IsosurfaceExtractor::new(isolevel);

        let pass_1_results = fe.pass_1(&gd);

        let metadata = IsosurfaceExtractor::pass_2(size, &pass_1_results.0, pass_1_results.1);
        let metadata = IsosurfaceExtractor::pass_3(metadata, size);
        println!("{:#?}", metadata.0);
    }
    #[test]
    fn pass_4() {
        let (map, isolevel, size) = test_map_3();

        let gd = GridBuffer { grid: map, size };

        let fe = IsosurfaceExtractor::new(isolevel);

        let pass_1_results = fe.pass_1(&gd);

        let metadata = IsosurfaceExtractor::pass_2(size, &pass_1_results.0, pass_1_results.1);
        let metadata = IsosurfaceExtractor::pass_3(metadata, size);

        let output = IsosurfaceExtractor::pass_4(size, metadata.0, metadata.1, metadata.2, &pass_1_results.0);
        println!("{:#?}", output);
    }
    #[test]
    fn pass_final() {
        let (map, isolevel, size) = test_map_3();

        let gd = GridBuffer { grid: map, size };

        let fe = IsosurfaceExtractor::new(isolevel);

        let pass_1_results = fe.pass_1(&gd);

        let metadata = IsosurfaceExtractor::pass_2(size, &pass_1_results.0, pass_1_results.1);
        let metadata = IsosurfaceExtractor::pass_3(metadata, size);

        let output = IsosurfaceExtractor::pass_4(size, metadata.0, metadata.1, metadata.2, &pass_1_results.0);
        

        let _final_output = IsosurfaceExtractor::final_output(output.0, output.1);
    }
    #[test]
    fn triangle_count() {
        let mut array: [u8; 256] = [0; 256];

        for (i, x) in TRIANGLE_TABLE.iter().enumerate() {
            let mut index = 0;

            while x[index] != -1 {
                index += 1;
            }
            array[i] = index as u8 % 3;
            println!("{}", index);
            assert!(index % 3 == 0)
        }
        println!("{}", array == TRIANGLE_COUNT);
    }
    #[test]
    fn tris() {
        for i in TRIANGLE_COUNT {
            println!("{:?}", i);
        }
    }
    
    
    #[test]
    fn edge_intersects() {
        let case_index: usize = 17;

        println!("{:16b}", EDGE_INTERSECTION[case_index]);
    }
}
