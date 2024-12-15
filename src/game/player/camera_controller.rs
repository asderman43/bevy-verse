use avian3d::parry::na::clamp;
use bevy::{
    input::mouse::MouseMotion,
    math::VectorSpace,
    prelude::*,
    render::camera,
    window::{CursorGrabMode, PrimaryWindow},
};

use super::{super::window, player::Player};

#[derive(Component)]
pub struct CameraController {
    pub rotation: Vec2,
    pub rotation_lock: f32,
    pub sensitivity: f32,
    pub length: f32,
    pub offset: Vec3,
    pub anchor: Vec3,
}

pub fn update_camera_controller(
    mut mouse_motion: EventReader<MouseMotion>,
    mut camera_query: Query<(&mut CameraController, &mut Transform), Without<Player>>,
    player_query: Query<(&Player, &Transform), With<Player>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
) {
    let window = window_query.get_single().unwrap();

    if let Ok((mut camera_controller, mut transform)) = camera_query.get_single_mut() {
        if window.cursor_options.grab_mode == CursorGrabMode::Locked {
            for ev in mouse_motion.read() {
                camera_controller.rotation.y -= ev.delta.x * camera_controller.sensitivity;
                camera_controller.rotation.x -= ev.delta.y * camera_controller.sensitivity;

                camera_controller.rotation.x = clamp(
                    camera_controller.rotation.x,
                    -camera_controller.rotation_lock,
                    camera_controller.rotation_lock,
                );
            }
        }
        let y_quat = Quat::from_axis_angle(Vec3::Y, camera_controller.rotation.y);
        let x_quat: Quat = Quat::from_axis_angle(Vec3::X, camera_controller.rotation.x);
        let player_position =
            if let Ok((player_character, player_transform)) = player_query.get_single() {
                player_transform.translation
            } else {
                Vec3::ZERO
            };
        transform.rotation = y_quat * x_quat;
        camera_controller.anchor = player_position;
        transform.translation = camera_controller.anchor
            + camera_controller.offset
            + transform
                .rotation
                .mul_vec3(Vec3::new(0., 0., camera_controller.length));
    }
}
