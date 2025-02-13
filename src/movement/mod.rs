pub mod algorithms;
use algorithms::*;
use std::f32::NAN;

use avian3d::{math::*, prelude::*};
use bevy::{
    ecs::query::Has,
    math::{vec3, NormedVectorSpace, VectorSpace},
    prelude::*,
    render::camera,
    state::commands,
    transform,
    utils::dbg,
};
pub mod components;
use components::*;
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
                //test_debug,
                movement,
            )
                .chain(),
        );
    }
}
#[derive(Event, Debug)]
pub enum MovementAction {
    Move(Vector2),
    Jump,
}

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
    movement: Movement,
    state: KCCStateComponent,
}

pub enum CharacterShape {
    /// Radius, Height
    Capsule(f32, f32),
}
impl CharacterShape {
    pub fn collider(&self) -> Collider {
        match *self {
            CharacterShape::Capsule(radius, height) => Collider::capsule(radius, height),
        }
    }
    pub fn caster_shape(&self) -> Collider {
        match *self {
            CharacterShape::Capsule(radius, height) => {
                Collider::capsule(radius - SKIN_WIDTH, height)
            }
        }
    }
}

impl CharacterControllerBundle {
    pub fn new(shape: CharacterShape, gravity: Vec3) -> Self {
        // Create shape caster as a slightly smaller version of collider
        Self {
            character_controller: CharacterController {
                acceleration: 15.,
                jump_impulse: 15.0,
                max_slope: 60.0_f32.to_radians(),
                is_grounded: false,
                caster_shape: shape.caster_shape(),
            },
            velocity: Velocity::default(),
            movement: Movement(Vec3::ZERO),
            rigid_body: RigidBody::Kinematic,
            collider: shape.collider(),
            gravity: GravityController::from(gravity),
            state: KCCStateComponent::default(),
        }
    }
}
#[derive(Component)]
pub struct CharacterController {
    pub acceleration: f32,
    pub jump_impulse: f32,
    pub max_slope: f32,
    pub is_grounded: bool,
    pub caster_shape: Collider,
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

    movement_event_writer.send(MovementAction::Move(direction));

    if keyboard_input.just_pressed(KeyCode::Space) {
        movement_event_writer.send(MovementAction::Jump);
    }
}

fn update_grounded(
    spatial_query: SpatialQuery,
    mut player: Query<(
        Entity,
        &mut Transform,
        &mut CharacterController,
        &Collider,
        &mut KCCStateComponent,
    )>,
    mut commands: Commands,
) {
    for (entity, mut transform, mut cc, collider, mut state) in &mut player {
        // The character is grounded if the shape caster has a hit with a normal
        // that isn't too steep.
        //let is_grounded = true;
        let query_filter = SpatialQueryFilter::default().with_excluded_entities([entity]);

        if let Some(hit_data) = spatial_query.cast_shape(
            collider,
            transform.translation,
            Quaternion::IDENTITY,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(GROUND_DISTANCE),
            &query_filter,
        ) {
            cc.is_grounded = true;
            if hit_data.normal1.dot(Vec3::Y) > cc.max_slope.cos() {
                state.state = KCCState::Grounded(hit_data.normal1);
            } else {
                state.state = KCCState::Sliding(hit_data.normal1);
            }
            let snap = snap_to_floor(
                transform.translation,
                collider,
                0.04,
                cc.max_slope,
                &spatial_query,
                &query_filter,
            );

            transform.translation += snap;
        } else {
            cc.is_grounded = false;
            state.state = KCCState::InAir;
        }
    }
}
fn wish_movement(
    time: Res<Time>,
    mut movement_event_reader: EventReader<MovementAction>,
    mut controllers: Query<(
        &CharacterController,
        &mut Velocity,
        &KCCStateComponent,
        &mut Movement,
    )>,
    camera_query: Query<&Transform, With<Camera>>,
) {
    // Precision is adjusted so that the example works with
    // both the `f32` and `f64` features. Otherwise you don't need this.
    let delta_time = time.delta_secs_f64().adjust_precision();
    let camera = camera_query.single();
    for event in movement_event_reader.read() {
        for (controller, mut velocity, state, mut movement) in &mut controllers {
            let basis = if let KCCState::Grounded(floor_normal) = state.state {
                floor_normal
            } else {
                Vec3::Y
            };

            match event {
                MovementAction::Move(direction) => {
                    let input_dir = vec3(
                        direction.x * controller.acceleration,
                        0.0,
                        -direction.y * controller.acceleration,
                    );
                    movement.0 = (camera.rotation * input_dir)
                        .reject_from_normalized(basis)
                        .normalize_or_zero()
                        * controller.acceleration;

                    // velocity.0.x = direction.x * camera.left() * controller.acceleration;
                    // velocity.0.z = direction.y * camera.forward() * controller.acceleration;
                }
                MovementAction::Jump => {
                    println!("repulj!");
                    velocity.0.y += controller.jump_impulse;
                }
            }
            //println!("{:?}", velocity);
        }
    }
}
// TODO: Stick to the ground when close.
fn apply_gravity(
    time: Res<Time>,
    mut controllers: Query<(
        &GravityController,
        &mut Velocity,
        &CharacterController,
        &KCCStateComponent,
    )>,
) {
    // Precision is adjusted so that the example works with
    // both the `f32` and `f64` features. Otherwise you don't need this.
    let delta_time = time.delta_secs_f64().adjust_precision();

    for (gravity, mut velocity, controller, state) in &mut controllers {
        if KCCState::InAir == state.state {
            println!("gravity");
            velocity.0.y = velocity.0.y - gravity.length.powf(2.0) * delta_time;
        } else {
            println!("ground");
            velocity.0.y = 0.0;
        }
        //dbg!(state);
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
        &KCCStateComponent,
        &Movement,
    )>,
) {
    let delta_time = time.delta_secs_f64().adjust_precision();

    for (entity, mut transform, controller, mut velocity, collider, state, movement) in
        player.iter_mut()
    {
        //let collider_stats = collider.shape().as_capsule().unwrap();
        //let caster_stats = controller.caster_shape.shape().as_capsule().unwrap();

        //let wasd = transform.translation - vec3(0., collider_stats.height(), 0.);
        //gizmos.line(wasd, wasd + vec3(0., SKIN_WIDTH, 0.), Color::srgb(1.0, 1.0, 0.0));

        let query_filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
        let (new_pos, _) = collide_and_slide(
            &spatial_query,
            &query_filter,
            &controller.caster_shape,
            &transform,
            (velocity.0 + movement.0) * delta_time,
        )
        .get_last();
        //dbg!(new_pos);
        transform.translation = new_pos;
        //velocity.0 = vel / delta_time;
    }
}
fn step_up() {}

//TODO: Implement a skin-width version if there is some bugs. Basically scale the collider by 0.8, calculate the skin-width and substract that from the distance traveled.
