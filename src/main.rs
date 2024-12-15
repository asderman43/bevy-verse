use avian3d::prelude::*;
use bevy::prelude::*;
mod game;
fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            game::GamePlugin,
            PhysicsPlugins::default(),
            PhysicsDebugPlugin::default(),
        ))
        .run();
}
