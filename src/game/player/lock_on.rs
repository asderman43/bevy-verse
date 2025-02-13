use super::super::target::Target;
use bevy::prelude::*;
pub struct LockOnPlugin;
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, States)]
enum CameraState {
    #[default]
    Free,
    LockOn,
}

impl Plugin for LockOnPlugin {
    fn build(&self, app: &mut App) {}
}
#[derive(Component)]
pub struct LockOn {
    pub target: Entity,
    pub offset: Vec3,
    pub length: f32,
    pub rotation: Vec2,
}
// TODO: Implement the `select_target` system
pub fn select_target(
    target_query: Query<(&Transform, Entity), With<Target>>,
    mut lock_on_query: Query<&mut LockOn>,
) {
    for mut lock_on in lock_on_query.iter_mut() {
        for target in target_query.iter() {
            lock_on.target = target.1;
            break;
        }
    }
}
