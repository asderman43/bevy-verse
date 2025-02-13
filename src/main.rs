use avian3d::prelude::*;
use bevy::{
    dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin},
    prelude::*,
    text::FontSmoothing,
};
use bevy_panorbit_camera::{self, PanOrbitCamera, PanOrbitCameraPlugin};
use bevy_polyline::PolylinePlugin;
use bevy_verse::terrain::{
    self, debug::{draw_box, draw_points, keyboard_input}, update_mesh, Terrain 
};
use bevy_verse::terrain::voxel::Voxel;
mod game;
pub fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Import the custom texture.
    // Create and save a handle to the mesh.
    let size = 2;
    let real_size = size + 1;
    let mut v = Voxel::new(size);
    v.set((2, 0, 0), real_size, 1);
    v.set((1, 0, 0), real_size, 1);
    v.set((0, 0, 0), real_size, 1);
    v.set((1, 1, 0), real_size, 1);

    let cube_mesh_handle: Handle<Mesh> = meshes.add(v.create_mesh(size, real_size));

    // Render the mesh with the custom texture, and add the marker.
    commands.spawn((
        Mesh3d(cube_mesh_handle),
        MeshMaterial3d(materials.add(StandardMaterial {
            cull_mode: None,
            ..default()
        })),
    ));

    // Transform for the camera and lighting, looking at (0,0,0) (the position of the mesh).
    let camera_and_light_transform =
        Transform::from_xyz(1.8, 1.8, 1.8).looking_at(Vec3::ZERO, Vec3::Y);

    // Camera in 3D space.
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));

    // Light up the scene.
    commands.spawn((PointLight::default(), camera_and_light_transform));

    // Text to describe the controls.
}





fn setup_2(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain: ResMut<Terrain>,
) {
    let _ = terrain.set(IVec3::new(0, 1, 0), 1);
    let _ = terrain.set(IVec3::new(1, 2, 0), 1);
    let _ = terrain.set(IVec3::new(1, 1, 1), 1);

    terrain.create_mesh(&mut meshes);
    let handle = terrain.mesh_handle.clone().unwrap();
    commands.spawn((
        Mesh3d(handle),
        MeshMaterial3d(materials.add(StandardMaterial {
            cull_mode: None,
            ..default()
        })),
    ));


    



    let camera_and_light_transform =
        Transform::from_xyz(1.8, 1.8, 1.8).looking_at(Vec3::ZERO, Vec3::Y);

    // Camera in 3D space.
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));

    // Light up the scene.
    commands.spawn((PointLight::default(), camera_and_light_transform));
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, PanOrbitCameraPlugin, PolylinePlugin))
        .insert_resource(Terrain::new(2))
        .add_systems(Startup, (draw_box, setup_2))
        .add_systems(Update, (draw_points, keyboard_input))
        .add_systems(PostUpdate, update_mesh)
        .run();
}
// fn main() {
//     App::new()
//         .add_plugins((
//             DefaultPlugins,
//             game::GamePlugin,
//             PhysicsPlugins::default(),
//             FpsOverlayPlugin {
//                 config: FpsOverlayConfig {
//                     text_config: TextFont {
//                         // Here we define size of our overlay
//                         font_size: 42.0,
//                         // If we want, we can use a custom font
//                         font: default(),
//                         // We could also disable font smoothing,
//                         font_smoothing: FontSmoothing::default(),
//                     },
//                     // We can also change color of the overlay
//                     text_color: Color::srgb(0.0, 1.0, 0.0),
//                     enabled: true,
//                 },
//             },
//             // PhysicsDebugPlugin::default(),
//         ))
//         .run();
// }
