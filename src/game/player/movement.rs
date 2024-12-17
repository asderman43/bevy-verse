
use avian3d::{math::*, prelude::*};
use bevy::{
    ecs::query::Has, math::{vec3, NormedVectorSpace, VectorSpace}, prelude::*, transform, utils::dbg
};

pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<MovementAction>().add_systems(
            Update,
            (
                keyboard_input,
                update_grounded,
                apply_gravity,
                wish_movement,
                test_debug,
                movement,
                
            )
                .chain()
        );
        

    }
}
#[derive(Event, Debug)]
pub enum MovementAction { 
    Move(Vector2),
    Jump,
}
#[derive(Component)]
pub struct GravityController {
    /// Direction + Acceleration
    force: Vec3,
}



#[derive(Component)]
struct Grounded;

#[derive(Component, Debug)]
pub struct Velocity(Vec3);

impl Default for Velocity {
    fn default() -> Self {
        Self(Vec3::ZERO)
    }
}
#[derive(Bundle)]
pub struct CharacterControllerBundle {
    character_controller: CharacterController,
    rigid_body: RigidBody,
    collider: Collider,
    velocity: Velocity,
    gravity: GravityController,
}

pub enum CharacterShape {
    /// Radius, Height
    Capsule(f32, f32)
}
impl CharacterShape {
    pub fn collider(&self) -> Collider {
        match *self {
            CharacterShape::Capsule(radius, height) => Collider::capsule(radius, height)
        }
    }
    pub fn caster_shape(&self) -> Collider {
        match *self {
            CharacterShape::Capsule(radius, height) => Collider::capsule(radius - SKIN_WIDTH, height)
        }
    }
}

impl CharacterControllerBundle {
    pub fn new(shape: CharacterShape, gravity: Vector) -> Self {
        // Create shape caster as a slightly smaller version of collider
        Self {
            character_controller: CharacterController {
                acceleration: 15.,
                jump_impulse: 10.0,
                max_slope: 60.0_f32.to_radians(),
                is_grounded: false,
                caster_shape: shape.caster_shape()
            },
            velocity: Velocity::default(),
            rigid_body: RigidBody::Kinematic,
            collider: shape.collider(),
            gravity: GravityController { force: gravity },
        }
    }
}
#[derive(Component)]
pub struct CharacterController {
    pub acceleration: f32,
    pub jump_impulse: f32,
    pub max_slope: f32,
    pub is_grounded: bool,
    pub caster_shape: Collider
}

fn keyboard_input(
    mut movement_event_writer: EventWriter<MovementAction>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    let up = keyboard_input.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]);
    let down = keyboard_input.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]);
    let left = keyboard_input.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]);
    let right = keyboard_input.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]);

    let horizontal = right as i8 - left as i8;
    let vertical = up as i8 - down as i8;
    let direction = Vector2::new(horizontal as Scalar, vertical as Scalar).clamp_length_max(1.0);

    if direction != Vector2::ZERO {
        movement_event_writer.send(MovementAction::Move(direction));
    }

    if keyboard_input.just_pressed(KeyCode::Space) {
        movement_event_writer.send(MovementAction::Jump);
    }
}

fn update_grounded(
    mut query: Query<(Entity, &Rotation, &mut CharacterController), With<CharacterController>>,
) {
    for (entity, rotation, mut controller) in &mut query {
        // The character is grounded if the shape caster has a hit with a normal
        // that isn't too steep.
        //let is_grounded = true;

        //controller.is_grounded = is_grounded;
        //println!("{}", is_grounded);
    }
}
fn wish_movement(
    time: Res<Time>,
    mut movement_event_reader: EventReader<MovementAction>,
    mut controllers: Query<(&CharacterController, &mut Velocity)>,
) {
    // Precision is adjusted so that the example works with
    // both the `f32` and `f64` features. Otherwise you don't need this.
    let delta_time = time.delta_secs_f64().adjust_precision();

    for event in movement_event_reader.read() {
        for (controller, mut velocity) in &mut controllers {
            match event {
                MovementAction::Move(direction) => {
                    velocity.0.x = direction.x * controller.acceleration;
                    velocity.0.z = direction.y * controller.acceleration;
                }
                MovementAction::Jump => {
                    println!("repulj!");
                    velocity.0.y = controller.jump_impulse;
                }
            }
            //println!("{:?}", velocity);
        }
    }
}

fn apply_gravity(
    time: Res<Time>,
    mut controllers: Query<(&GravityController, &mut Velocity, &CharacterController)>,
) {
    // Precision is adjusted so that the example works with
    // both the `f32` and `f64` features. Otherwise you don't need this.
    let delta_time = time.delta_secs_f64().adjust_precision();

    for (gravity, mut velocity, controller) in &mut controllers {
        if !controller.is_grounded {
            velocity.0.y -= gravity.force.length() * delta_time;
        }
    }
}
fn movement(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    mut player: Query<(
        Entity,
        &mut Transform,
        &CharacterController,
        &mut Velocity,
        &Collider,
        
    )>,
    mut gizmos: Gizmos,
) {
    let delta_time = time.delta_secs_f64().adjust_precision();
    
    for (entity, mut transform, controller, mut velocity, collider) in player.iter_mut() {
        let query_filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
        let (new_pos, vel) = collide_and_slide(
            &spatial_query,
            &query_filter,
            &controller.caster_shape,
            &transform,
            velocity.0 * delta_time
        ).get_last();
        transform.translation = new_pos;
        //velocity.0 = vel / delta_time;
        //velocity.0 = Vec3::ZERO;
        // gizmos.primitive_3d(
        //     &Capsule3d::new(1.0, 2.0),
        //     Isometry3d::new(transform.translation, Quat::IDENTITY),
        //     Color::srgb(1.0, 0.0, 0.0),
        // );
        velocity.0 = Vec3::ZERO;
    }
    
}


pub struct Bounce {
    /// Position and Velocity
    bounces: Vec<(Vec3, Vec3)>
}
const MAX_BOUNCES: u8 = 3;
const EPSILON: f32 = 0.005;
const SKIN_WIDTH: f32 = 0.5;
impl Bounce {
    pub fn new() -> Bounce {
        return Bounce { bounces: Vec::with_capacity(MAX_BOUNCES as usize) }
    }
    pub fn add(&mut self, position: Vec3, velocity: Vec3) {
        self.bounces.push((position, velocity));
    }
    pub fn get(self) -> Vec<(Vec3, Vec3)> {
        self.bounces
    }
    pub fn get_last(self) -> (Vec3, Vec3) {
        self.bounces.last().copied().unwrap_or((Vec3::ZERO, Vec3::ZERO))
    }
}
//TODO: Implement a skin-width version if there is some bugs. Basically scale the collider by 0.8, calculate the skin-width and substract that from the distance traveled.
fn collide_and_slide(
    spatial_query: &SpatialQuery,
    filter: &SpatialQueryFilter,
    collider: &Collider,
    transform: &Transform,
    _velocity: Vec3
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
            velocity = Vec3::ZERO;
            bounces.add(position, velocity);
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
            filter
        ) {
            // The remaining velocity, past the rayhit;
            let remaining = length - hit_data.distance;
            let safe_movement = direction * (hit_data.distance - EPSILON).max(0.0) + hit_data.normal1 * EPSILON;
             transform.up();
            position += safe_movement;
            velocity -= safe_movement;

            let project_on_plane = velocity.reject_from_normalized(hit_data.normal1).normalize();
            velocity = project_on_plane * remaining * f32::sqrt(direction.dot(hit_data.normal1) + 1.);
            bounces.add(position, velocity);
        } else {
            bounces.add(position + velocity, Vec3::ZERO);
            return bounces;
        }
        
    }
    //println!("{}", rem_vel);
    return bounces;
}
pub fn test_debug(
    mut gizmos: Gizmos,
    spatial_query: SpatialQuery,
    player: Query<(
        Entity,
        &Transform,
        &CharacterController,
        &Velocity,
        &Collider,
    )>,) {
        for (entity, transform, controller, velocity, collider) in player.iter() {
            let query_filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
            
            let bounces = collide_and_slide(
                &spatial_query,
                &query_filter,
                collider,
                &transform,
                velocity.0,
            ).get();
            gizmos.arrow(
                transform.translation,
                transform.translation + velocity.0,
                Color::srgb(0.0, 0.0, 1.0),
            );

            let collider_stats = collider.shape().as_capsule().unwrap();
            let caster_stats = controller.caster_shape.shape().as_capsule().unwrap();

            gizmos.primitive_3d(
                &Capsule3d::new(collider_stats.radius, collider_stats.height()),
                Isometry3d::new(transform.translation, Quat::IDENTITY),
                Color::srgb(1.0, 0.0, 1.0),
            );
            gizmos.primitive_3d(
                &Capsule3d::new(caster_stats.radius, caster_stats.height()),
                Isometry3d::new(transform.translation, Quat::IDENTITY),
                Color::srgb(1.0, 1.0, 1.0),
            );

            let mut color = 0.0;
            for (pos, vel) in bounces {
                let (direction, length) = if let Ok(val) = Dir3::new_and_length(vel) {
                    val
                } else {
                    break;
                };
                gizmos.arrow(
                    pos,
                    pos + direction * length,
                    Color::srgb(0.0, 0.0, 1.0),
                );
                gizmos.primitive_3d(
                    &Capsule3d::new(caster_stats.radius, caster_stats.height()),
                    Isometry3d::new(pos, Quat::IDENTITY),
                    Color::srgb(1.0 - color, 0.0, color),
                );
                color += 0.25;
                
            }

            //velocity.0 = Vec3::ZERO;
        }
}
