use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;

pub struct PrototypePlugin;

impl Plugin for PrototypePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init);
    }
}

fn init(mut commands: Commands) {
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        PanOrbitCamera::default(),
    ));
    
    commands.insert_resource(AmbientLight {
        brightness: 1000.0,
        ..default()
    });
}