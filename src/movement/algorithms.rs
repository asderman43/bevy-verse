use avian3d::{math::Quaternion, prelude::*};
use bevy::prelude::*;
pub const FLOOR_EPSILON: f32 = 0.05;
pub const MAX_BOUNCES: u8 = 3;
pub const EPSILON: f32 = 0.001;
pub const SKIN_WIDTH: f32 = 0.001;
pub const GROUND_DISTANCE: f32 = 0.1;
pub fn snap_to_floor(
    origin: Vec3,
    collider: &Collider,
    floor_snap_length: f32,
    _max_slide_angle: f32,
    spatial_query: &SpatialQuery,
    filter: &SpatialQueryFilter,
) -> Vec3 {
    if let Some(hit) = spatial_query.cast_shape(
        collider,
        origin,
        Quat::IDENTITY, // TODO: Support rotation?
        Dir3::NEG_Y,
        &ShapeCastConfig::from_max_distance(floor_snap_length + FLOOR_EPSILON), // TODO: Don't hardcode this value
        filter,
    ) {
        return (hit.distance - FLOOR_EPSILON * hit.normal2.angle_between(Vec3::NEG_Y).cos())
            .max(0.)
            * Vec3::NEG_Y;
    }

    Vec3::ZERO
}
pub struct Bounce {
    /// Position and Velocity
    bounces: Vec<(Vec3, Vec3)>,
}

impl Bounce {
    pub fn new() -> Bounce {
        return Bounce {
            bounces: Vec::with_capacity(MAX_BOUNCES as usize),
        };
    }
    pub fn add(&mut self, position: Vec3, velocity: Vec3) {
        self.bounces.push((position, velocity));
    }
    pub fn get(self) -> Vec<(Vec3, Vec3)> {
        self.bounces
    }
    pub fn read(&self) -> &Vec<(Vec3, Vec3)> {
        &self.bounces
    }
    pub fn get_last(self) -> (Vec3, Vec3) {
        self.bounces
            .last()
            .copied()
            .unwrap_or((Vec3::ZERO, Vec3::ZERO))
    }
}
pub fn collide_and_slide(
    spatial_query: &SpatialQuery,
    filter: &SpatialQueryFilter,
    collider: &Collider,
    transform: &Transform,
    _velocity: Vec3,
) -> Bounce {
    let mut bounces = Bounce::new();

    // Remaining velocity and direction;
    let mut velocity = _velocity;
    // The position from which we are raycasting, initally the character's position;
    let mut position = transform.translation;

    for _ in 0..=MAX_BOUNCES {
        let (direction, length) = if let Ok(val) = Dir3::new_and_length(velocity) {
            val
        } else {
            bounces.add(position, Vec3::ZERO);
            break;
        };
        let max_distance = length;

        //println!("{:?} {}", position, velocity);
        if let Some(hit_data) = spatial_query.cast_shape(
            collider,
            position,
            Quaternion::IDENTITY,
            direction,
            &ShapeCastConfig::from_max_distance(max_distance),
            filter,
        ) {
            // The remaining velocity, past the rayhit;
            //dbg!(hit_data.distance, position);

            let safe_movement = direction * hit_data.distance; //+ hit_data.normal1 * EPSILON;
            position += safe_movement + hit_data.normal1 * EPSILON;
            velocity -= safe_movement;

            let project_on_plane = velocity.reject_from_normalized(hit_data.normal1);
            velocity = project_on_plane; //* remaining * f32::sqrt(direction.dot(hit_data.normal1) + 1.);

            bounces.add(position, velocity);
        } else {
            bounces.add(position + velocity, Vec3::ZERO);
            break;
        }
    }
    //dbg!(bounces.read());
    //println!("{}", rem_vel);
    return bounces;
}
