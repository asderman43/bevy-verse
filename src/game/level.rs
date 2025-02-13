use avian3d::{parry::transformation::utils::transform, prelude::*};
use bevy::{
    math::{vec3, VectorSpace},
    prelude::*,
    text::cosmic_text::Angle,
};

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_level);
    }
}
fn init_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let level_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        ..Default::default()
    });
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(30., 30., 30.),
        Mesh3d(meshes.add(Cuboid::new(30., 30., 30.))),
        MeshMaterial3d(level_material.clone()),
        Transform::from_xyz(0., 0., -100.),
    ));
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(30., 30., 30.),
        Mesh3d(meshes.add(Cuboid::new(30., 30., 30.))),
        MeshMaterial3d(level_material.clone()),
        Transform::from_xyz(0., 0., -60.),
    ));
    commands.spawn((
        RigidBody::Dynamic,
        Collider::cuboid(3.0, 3.0, 3.0),
        Mesh3d(meshes.add(Cuboid::from_length(3.0))),
        MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
        Transform::from_xyz(-20.0, 4.0, -80.0),
    ));
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(1000., 1., 1000.),
        Mesh3d(meshes.add(Cuboid::new(1000., 1., 1000.))),
        MeshMaterial3d(level_material.clone()),
        Transform::IDENTITY,
    ));

    let stair = commands.spawn(Transform::from_xyz(-20.0, 0.0, -100.0)).id();

    for x in 0..=10 {
        let stair_child = commands
            .spawn((
                RigidBody::Static,
                Collider::cuboid(5., 1., 1.),
                Mesh3d(meshes.add(Cuboid::new(5., 1., 1.))),
                MeshMaterial3d(level_material.clone()),
                Transform::from_xyz(0., x as f32 + 1.0, -x as f32),
            ))
            .id();
        commands.entity(stair).add_child(stair_child);
        //commands.entity(stair);
    }

    // let stair_1 = commands.spawn((RigidBody::Static,
    //     Collider::cuboid(5., 1., 1.),
    //     Mesh3d(meshes.add(Cuboid::new(5., 1., 1.))),
    //     MeshMaterial3d(level_material.clone()),
    //     Transform::from_xyz(0.0, 1.0, 0.0))
    // ).id();
    // let stair_2 = commands.spawn((RigidBody::Static,
    //     Collider::cuboid(5., 1., 1.),
    //     Mesh3d(meshes.add(Cuboid::new(5., 1., 1.))),
    //     MeshMaterial3d(level_material.clone()),
    //     Transform::from_xyz(1.0, 2.0, 0.0))
    // ).id();
    // let stair_3 = commands.spawn((RigidBody::Static,
    //     Collider::cuboid(5., 1., 1.),
    //     Mesh3d(meshes.add(Cuboid::new(5., 1., 1.))),
    //     MeshMaterial3d(level_material.clone()),
    //     Transform::from_xyz(1.0, 2.0, 0.0))
    // ).id();

    // commands.entity(stair).add_children(&[stair_1, stair_2, stair_3]);
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(10., 10., 10.),
        Mesh3d(meshes.add(Cuboid::new(10., 10., 10.))),
        MeshMaterial3d(level_material.clone()),
        Transform::from_matrix(Mat4::from_rotation_translation(
            Quat::from_rotation_z(45.0_f32.to_radians()),
            Vec3::new(20., 0., -80.),
        )),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            shadows_enabled: true,
            ..Default::default()
        },
        Transform::from_xyz(100., 200., 100.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
