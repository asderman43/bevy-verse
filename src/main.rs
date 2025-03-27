use std::f32::consts::PI;

use avian3d::prelude::*;
use bevy::{
    color::palettes::css::ORANGE_RED,
    dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin},
    pbr::CascadeShadowConfigBuilder,
    prelude::*,
    remote::{http::RemoteHttpPlugin, RemotePlugin},
    text::FontSmoothing,
};
use bevy_panorbit_camera::{self, PanOrbitCamera, PanOrbitCameraPlugin};
use bevy_polyline::PolylinePlugin;
use bevy_verse::voxel_terrain::{
    debug::point_cloud::PointCloudPlugin,
    noise::noise_preview,
    terrain::{update_mesh, Terrain},
};
use libnoise::Generator;

mod game;

fn setup_2(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain: ResMut<Terrain>,
) {
    terrain.generate_chunk(IVec3::ZERO);
    terrain.generate_chunk(IVec3::X);
    terrain.generate_chunk(IVec3::Z);
    terrain.generate_chunk(IVec3::NEG_X);
    terrain.generate_chunk(IVec3::NEG_Z);
    terrain.create_mesh(&mut meshes, &mut commands, &mut materials);
    
    // let handle = terrain.mesh_handle.clone().unwrap();
    // commands.spawn((
    //     Mesh3d(handle),
    //     MeshMaterial3d(materials.add(StandardMaterial {

    //         ..default()
    //     })),
    // ));

    let camera_and_light_transform =
        Transform::from_xyz(1.8, 1.8, 1.8).looking_at(Vec3::ZERO, Vec3::Y);

    // Camera in 3D space.
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));

    // Light up the scene.
    commands.insert_resource(AmbientLight {
        color: ORANGE_RED.into(),
        brightness: 100.,
    });

    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadows_enabled: true,
            ..default()
        },
        Transform {
            translation: Vec3::new(0.0, 2.0, 0.0),
            rotation: Quat::from_rotation_x(-PI / 4.),
            ..default()
        },
        // The default cascade config is designed to handle large scenes.
        // As this example has a much smaller world, we can tighten the shadow
        // bounds for better visual quality.
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 4.0,
            maximum_distance: 10.0,
            ..default()
        }
        .build(),
    ));
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PolylinePlugin,
            PointCloudPlugin,
        ))
        .add_plugins(RemotePlugin::default())
        .add_plugins(RemoteHttpPlugin::default())
        .insert_resource(Terrain::new(2, 2))
        .add_systems(Startup, (setup_2, noise_preview))
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
