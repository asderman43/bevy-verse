use bevy::{
    color::palettes::css::{BLUE, GREEN, RED, YELLOW_GREEN},
    math::{vec3, VectorSpace},
    prelude::*,
};
use bevy_polyline::prelude::*;



use super::{ util::{position_from_index, XYZ}, Terrain};

// TODO! ennek nem kell ennyire komplikaltnak lennie.
pub fn create_box(size: usize) -> Vec<(Vec3, Vec3)> {
    let line_count = (size + 1) * size.pow(2) + 3;
    let mut lines = Vec::with_capacity(line_count);
    let mut out_points = Vec::with_capacity(size.pow(3) * 8);

    for (x, y, z) in XYZ::new(size) {
        // Build cube
        //let (x,y, z) = (x as f32, y as f32, z as f32);
        let mut points: [Vec3; 8] = [Vec3::ZERO; 8];
        //println!("{:?}", (x, y, z));
        for i in 0..8 {
            points[i] = vec3(
                (x + ((i & 1) >> 0)) as f32,
                (y + ((i & 2) >> 1)) as f32,
                (z + ((i & 4) >> 2)) as f32,
            );
            out_points.push(points);
            // println!("{:?}", points);
            //println!("{} {} {}", (i & 1) >> 0, (i & 2) >> 1, (i & 4) >> 2);
        }
        if x == 0 {
            lines.push((points[0], points[4]));
            lines.push((points[4], points[6]));
            lines.push((points[6], points[2]));
            lines.push((points[0], points[2]));
        }
        if y == 0 {
            lines.push((points[0], points[1]));
            lines.push((points[1], points[5]));
            lines.push((points[5], points[4]));
            lines.push((points[4], points[0]));
        }
        if z == 0 {
            lines.push((points[0], points[1]));
            lines.push((points[1], points[3]));
            lines.push((points[3], points[2]));
            lines.push((points[2], points[0]));
        }

        lines.push((points[6], points[7]));
        lines.push((points[7], points[3]));
        lines.push((points[7], points[5]));
    }

    // (lines, out_points)
    lines
}
pub fn draw_points(mut gizmos: Gizmos, terrain: Res<Terrain>) {
    for (x, y, z) in XYZ::new(terrain.size) {
        let position = IVec3::new(x as i32, y as i32, z as i32);
        let c = terrain.get(position);

        for i in 0..8 {
            let pos = position_from_index(i);
            let pos = ((x + pos.0) as i32, (y + pos.1) as i32, (z + pos.2) as i32);
            let val = terrain.get(IVec3::new(pos.0, pos.1, pos.2));
            
            let color = if let Some(current) = val {
                if current == 0 {
                    RED
                } else {
                    GREEN
                }
            } else {
                RED
            };
            gizmos.sphere(vec3(pos.0 as f32, pos.1 as f32, pos.2 as f32), 0.1, color);


        }
        

        let position = Vec3::new(x as f32, y as f32, z as f32);
        

        
    }
    gizmos.arrow(Vec3::ZERO - Vec3::new(0.1, 0.1, 0.1), Vec3::Y - Vec3::new(0.1, 0.1, 0.1), GREEN);
    gizmos.arrow(Vec3::ZERO - Vec3::new(0.1, 0.1, 0.1), Vec3::X - Vec3::new(0.1, 0.1, 0.1), RED);
    gizmos.arrow(Vec3::ZERO - Vec3::new(0.1, 0.1, 0.1), Vec3::Z - Vec3::new(0.1, 0.1, 0.1), BLUE);
}

pub fn draw_box(
    mut commands: Commands,
    // mut gizmos: Gizmos
    terrain: Res<Terrain>,
    mut polyline_materials: ResMut<Assets<PolylineMaterial>>,
    mut polylines: ResMut<Assets<Polyline>>,
) {
    let lines = create_box(terrain.size);

    for line in lines {
        // println!("{:?}", line);
        commands.spawn(PolylineBundle {
            polyline: PolylineHandle(polylines.add(Polyline {
                vertices: vec![line.0, line.1],
            })),
            material: PolylineMaterialHandle(polyline_materials.add(PolylineMaterial {
                width: 2.0,
                color: RED.into(),
                perspective: false,
                // Bias the line toward the camera so the line at the cube-plane intersection is visible
                depth_bias: -0.0002,
            })),
            ..Default::default()
        });
    }
}





pub fn keyboard_input(
    mut terrain: ResMut<Terrain>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    let input = [KeyCode::Digit1 , KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];
    
        
        
    //let array = keyboard_input.get_just_pressed().filter(|key_code| input.contains(*key_code)).map(|key_code| input.binary_search(key_code).unwrap());
    let array = input.iter().enumerate().filter_map(|(i, key)| if keyboard_input.just_pressed(*key) { Some(i) } else { None });  
        for i in array {
            //println!("{} {} {}", (i & 1) >> 0, (i & 2) >> 1, (i & 4) >> 2);
            let (x, y, z) = position_from_index(i);
            let position: IVec3 = IVec3::new(x as i32, y as i32, z as i32);
            

            if let Some(voxel) = terrain.get(position) {
                let value = match voxel {
                    0 => 1,
                    1 => 0,
                    _ => 0
                };

                let _ = terrain.set(position, value);
            }
        }
        
    
    
}
