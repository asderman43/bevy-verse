use super::{
    camera_controller,
    movement::{CharacterController, CharacterControllerBundle, CharacterControllerPlugin},
};
use avian3d::{math::Quaternion, prelude::*};
use bevy::{
    math::{vec3, VectorSpace},
    prelude::*,
};
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, camera_controller::update_camera_controller);
        app.add_systems(Startup, init_player);
        app.add_plugins(CharacterControllerPlugin);
    }
}
#[derive(Component)]
pub struct Player {}

fn init_player(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let fov: f32 = 90.0_f32.to_radians();
    let character_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.0, 0.0),
        ..Default::default()
    });
    let camera_entity = commands
        .spawn((
            Camera3dBundle {
                transform: Transform::IDENTITY,
                projection: Projection::Perspective(PerspectiveProjection {
                    fov: fov,
                    ..Default::default()
                }),
                ..Default::default()
            },
            camera_controller::CameraController {
                rotation: Vec2::ZERO,
                rotation_lock: 90.0_f32.to_radians(),
                sensitivity: 0.35_f32.to_radians(),
                offset: vec3(0., 1.7, 0.),
                length: 10.,
                anchor: Vec3::ZERO,
            },
        ))
        .id();

    let player_entity = commands
        .spawn((
            Player {},
            CharacterControllerBundle::new(Collider::capsule(1., 2.0), vec3(0., -9.81, 0.)),
            Transform::from_xyz(0., 5., -80.),
            /*PbrBundle {
                material: character_material,
                transform: Transform::from_xyz(0., 5., -80.),
                mesh: meshes.add(Capsule3d::new(1., 2.0)),
                ..Default::default()
            },*/
            //DebugRender::default().with_collider_color(Color::srgb(1.0, 0.0, 0.0)),
        ))
        .id();
    commands.entity(player_entity);
    commands.entity(camera_entity);
}
