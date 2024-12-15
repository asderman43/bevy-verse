use bevy::{
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow, WindowMode, WindowResolution},
};

pub struct GameWindowPlugin;
#[derive(Resource, Default)]
pub struct Cursor {
    pub locked: bool,
}
impl Cursor {
    pub fn invert_lock(&mut self, window: &mut Mut<'_, Window>) {
        self.locked = !self.locked;
        window.cursor_options.visible = !self.locked;

        if self.locked {
            let window_width = window.width();
            let window_height = window.height();

            window.cursor_options.grab_mode = CursorGrabMode::Locked;
            window.set_cursor_position(Some(Vec2::new(window_width / 2.0, window_height / 2.0)));
        } else {
            window.cursor_options.grab_mode = CursorGrabMode::None;
        }
    }
}

fn update_cursor_locking(
    keys: Res<ButtonInput<KeyCode>>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
    mut cursor: ResMut<Cursor>,
) {
    let mut window = window_query.get_single_mut().unwrap();
    if keys.just_pressed(KeyCode::Escape) {
        cursor.invert_lock(&mut window);
    }
}
impl Plugin for GameWindowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cursor>();
        app.add_systems(Update, update_cursor_locking);
        app.add_systems(PreStartup, init_window);
    }
}

fn init_window(mut window_query: Query<&mut Window, With<PrimaryWindow>>) {
    if let Ok(mut window) = window_query.get_single_mut() {
        window.resolution = WindowResolution::new(1920., 1080.);
        window.mode = WindowMode::Windowed;
    }
}
