use avian3d::{math::*, prelude::*};
use bevy::prelude::*;

#[derive(Component, Debug, PartialEq)]
pub enum KCCState {
    /// The player is on the ground. Vec3 is the normal of the ground.
    Grounded(Vec3),
    /// The player is sliding. Vec3 is the normal of the ground.
    Sliding(Vec3),
    InAir,
}
#[derive(Component, Debug)]
pub struct KCCStateComponent {
    pub state: KCCState,
}
impl Default for KCCStateComponent {
    fn default() -> Self {
        KCCStateComponent {
            state: KCCState::InAir,
        }
    }
}

#[derive(Component, Debug)]
pub struct Velocity(pub Vec3);

#[derive(Component, Debug)]
pub struct MoveWish(pub Vec3);

#[derive(Component)]
pub struct GravityController {
    /// Direction + Acceleration
    direction: Dir3,
    pub length: f32,
}
impl From<Vec3> for GravityController {
    fn from(vec: Vec3) -> Self {
        let (direction, length) = if let Ok(val) = Dir3::new_and_length(vec) {
            val
        } else {
            (Dir3::NEG_Y, 0.0)
        };
        Self { direction, length }
    }
}
#[derive(Component)]
pub struct Movement(pub Vec3);
