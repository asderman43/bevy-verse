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
            
            let collider_stats = collider.shape().as_capsule().unwrap();
            let caster_stats = controller.caster_shape.shape().as_capsule().unwrap();

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