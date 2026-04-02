use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{HitFlash, HitPoints};
use crate::player::{Dodge, Player};
use crate::world::fog::FogDynamic;
use crate::world::tilemap::WallSpatialIndex;

#[derive(Component)]
pub struct Arrow {
    direction: Vec3,
    lifetime: f32,
}

#[derive(Resource)]
pub(super) struct ArrowMeshes {
    shaft: Handle<Mesh>,
    head: Handle<Mesh>,
    shaft_material: Handle<StandardMaterial>,
    head_material: Handle<StandardMaterial>,
}

type ArrowPlayer<'w> = Single<
    'w,
    (
        &'static Transform,
        &'static Dodge,
        &'static mut HitPoints,
        &'static mut HitFlash,
    ),
    (With<Player>, Without<Arrow>),
>;

const ARROW_SPEED: f32 = 6.0;
const ARROW_DAMAGE: i32 = 5;
const ARROW_LIFETIME: f32 = 4.0;
const ARROW_HIT_RADIUS: f32 = 0.5;
const ARROW_WALL_RADIUS: f32 = 0.08;
const ARROW_HIT_HEIGHT: f32 = 2.2;

pub(super) fn setup_arrow_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ArrowMeshes {
        shaft: meshes.add(Cylinder::new(0.05, 0.9).mesh().resolution(5)),
        head: meshes.add(Cone::new(0.10, 0.22).mesh().resolution(5)),
        shaft_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.40, 0.18),
            emissive: LinearRgba::new(0.3, 0.2, 0.05, 1.0),
            perceptual_roughness: 0.85,
            ..default()
        }),
        head_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.5, 0.5, 0.55),
            emissive: LinearRgba::new(0.2, 0.2, 0.25, 1.0),
            metallic: 0.2,
            perceptual_roughness: 0.70,
            ..default()
        }),
    });
}

pub(super) fn spawn_arrow(
    commands: &mut Commands,
    meshes: &ArrowMeshes,
    position: Vec3,
    direction: Vec3,
) {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir);

    commands
        .spawn((
            Arrow {
                direction: dir,
                lifetime: ARROW_LIFETIME,
            },
            FogDynamic::default(),
            Transform::from_translation(position).with_rotation(rotation),
            Visibility::Visible,
        ))
        .with_children(|parent| {
            // Shaft (oriented along local Z)
            parent.spawn((
                Mesh3d(meshes.shaft.clone()),
                MeshMaterial3d(meshes.shaft_material.clone()),
                Transform::from_rotation(Quat::from_rotation_x(PI / 2.0)),
            ));
            // Arrowhead at the front
            parent.spawn((
                Mesh3d(meshes.head.clone()),
                MeshMaterial3d(meshes.head_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.35).with_rotation(Quat::from_rotation_x(PI / 2.0)),
            ));
        });
}

pub(super) fn update_arrows(
    mut commands: Commands,
    time: Res<Time>,
    wall_index: Res<WallSpatialIndex>,
    mut player: ArrowPlayer<'_>,
    mut arrows: Query<(Entity, &mut Arrow, &mut Transform), Without<Player>>,
) {
    let delta = time.delta_secs();
    let (player_transform, player_dodge, ref mut player_health, ref mut player_flash) = *player;
    let player_pos = player_transform.translation;
    let player_ground = Vec2::new(player_pos.x, player_pos.z);

    for (entity, mut arrow, mut transform) in &mut arrows {
        arrow.lifetime -= delta;
        if arrow.lifetime <= 0.0 {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        let start = transform.translation;
        let end = start + arrow.direction * ARROW_SPEED * delta;
        let start_2d = Vec2::new(start.x, start.z);
        let end_2d = Vec2::new(end.x, end.z);

        let wall_hit = wall_index.first_hit_fraction(start_2d, end_2d, ARROW_WALL_RADIUS);
        let player_hit = player_contact_fraction(start, end, player_ground);

        match resolve_arrow_impact(wall_hit, player_hit, player_dodge.active) {
            Some(ArrowImpact::Wall(wall_t)) => {
                transform.translation = start + (end - start) * wall_t;
                commands.entity(entity).despawn_recursive();
            }
            Some(ArrowImpact::Player {
                fraction,
                damages_player,
            }) => {
                transform.translation = start + (end - start) * fraction;
                if damages_player && player_health.apply_damage(ARROW_DAMAGE) > 0 {
                    player_flash.trigger();
                }
                commands.entity(entity).despawn_recursive();
            }
            None => transform.translation = end,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ArrowImpact {
    Wall(f32),
    Player { fraction: f32, damages_player: bool },
}

fn player_contact_fraction(start: Vec3, end: Vec3, player_ground: Vec2) -> Option<f32> {
    if !(0.0..=ARROW_HIT_HEIGHT).contains(&start.y) {
        return None;
    }

    segment_circle_intersection_fraction(
        Vec2::new(start.x, start.z),
        Vec2::new(end.x, end.z),
        player_ground,
        ARROW_HIT_RADIUS,
    )
}

fn resolve_arrow_impact(
    wall_hit: Option<f32>,
    player_hit: Option<f32>,
    player_dodging: bool,
) -> Option<ArrowImpact> {
    match (wall_hit, player_hit) {
        (Some(wall_t), Some(player_t)) if wall_t <= player_t => Some(ArrowImpact::Wall(wall_t)),
        (_, Some(player_t)) => Some(ArrowImpact::Player {
            fraction: player_t,
            damages_player: !player_dodging,
        }),
        (Some(wall_t), None) => Some(ArrowImpact::Wall(wall_t)),
        (None, None) => None,
    }
}

fn segment_circle_intersection_fraction(
    start: Vec2,
    end: Vec2,
    center: Vec2,
    radius: f32,
) -> Option<f32> {
    let delta = end - start;
    let a = delta.length_squared();
    if a <= 0.0001 {
        return (start.distance_squared(center) <= radius * radius).then_some(0.0);
    }

    let offset = start - center;
    let b = 2.0 * offset.dot(delta);
    let c = offset.length_squared() - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt_discriminant = discriminant.sqrt();
    let t1 = (-b - sqrt_discriminant) / (2.0 * a);
    let t2 = (-b + sqrt_discriminant) / (2.0 * a);

    [t1, t2]
        .into_iter()
        .filter(|t| (0.0..=1.0).contains(t))
        .min_by(|a, b| a.total_cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dodging_player_blocks_arrows_without_taking_damage() {
        let impact = resolve_arrow_impact(None, Some(0.4), true);

        match impact {
            Some(ArrowImpact::Player {
                fraction,
                damages_player,
            }) => {
                assert_eq!(fraction, 0.4);
                assert!(!damages_player);
            }
            _ => panic!("expected a non-damaging player impact"),
        }
    }

    #[test]
    fn wall_impacts_win_when_they_happen_first() {
        let impact = resolve_arrow_impact(Some(0.2), Some(0.4), false);

        match impact {
            Some(ArrowImpact::Wall(fraction)) => assert_eq!(fraction, 0.2),
            _ => panic!("expected a wall impact"),
        }
    }

    #[test]
    fn high_arrows_do_not_contact_the_player() {
        let hit = player_contact_fraction(
            Vec3::new(0.0, ARROW_HIT_HEIGHT + 0.1, 0.0),
            Vec3::new(2.0, ARROW_HIT_HEIGHT + 0.1, 0.0),
            Vec2::new(1.0, 0.0),
        );

        assert_eq!(hit, None);
    }
}
