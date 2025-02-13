use bevy::prelude::*;
pub mod level;
pub mod player;
pub mod target;
pub mod window;
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            window::GameWindowPlugin,
            level::LevelPlugin,
            player::player::PlayerPlugin,
        ));
    }
}
