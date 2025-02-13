use bevy::prelude::*;

pub struct IdleState {
    pub idle_time: f32,
}

pub struct WalkingState {}
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum State {
    Idle,
    Walking,
}

#[derive(Event)]
pub struct StateChangeEvent {
    //entity: Entity,
    pub state: State,
}

pub struct RandomPlugin;

impl Plugin for RandomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, run_idle_states);
    }
}

pub fn run_idle_states(
    mut state_change: EventWriter<StateChangeEvent>,
    mut query: Query<&mut StateMachine>,
) {
    for mut state_machine in query.iter_mut() {
        if state_machine.get_state() == &State::Idle {
            println!("Idle");
            if true {
                state_change.send(StateChangeEvent {
                    state: State::Walking,
                });
                state_machine.update(State::Walking);
            }
        }
    }
}
pub fn run_walking_states(query: Query<&mut StateMachine>) {
    for state_machine in query.iter() {
        if state_machine.get_state() == &State::Walking {
            println!("Walking");
        }
    }
}

pub fn on_idle_to_walk_transition(mut query: Query<&mut StateMachine>) {
    for mut state_machine in query.iter_mut() {}
}

#[derive(Component)]
pub struct StateMachine {
    state: State,
}

impl StateMachine {
    pub fn new() -> Self {
        Self { state: State::Idle }
    }

    pub fn update(&mut self, new_state: State) {
        // Fire an event to notify the state change
        self.state = new_state;
    }

    pub fn get_state(&self) -> &State {
        &self.state
    }
}
