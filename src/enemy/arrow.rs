use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{HitFlash, HitPoints};
use crate::player::{Dodge, Player};

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
const ARROW_DAMAGE: i32 = 2;
const ARROW_LIFETIME: f32 = 4.0;
const ARROW_HIT_RADIUS: f32 = 0.5;
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

pub(super) fn spawn_arrow(commands: &mut Commands, meshes: &ArrowMeshes, position: Vec3, direction: Vec3) {
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
    mut player: ArrowPlayer<'_>,
    mut arrows: Query<(Entity, &mut Arrow, &mut Transform), Without<Player>>,
) {
    let delta = time.delta_secs();
    let (player_transform, player_dodge, ref mut player_health, ref mut player_flash) = *player;
    let player_pos = player_transform.translation;

    for (entity, mut arrow, mut transform) in &mut arrows {
        arrow.lifetime -= delta;
        if arrow.lifetime <= 0.0 {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        // Move arrow
        transform.translation += arrow.direction * ARROW_SPEED * delta;

        // Check collision with player
        let to_player = Vec3::new(
            player_pos.x - transform.translation.x,
            0.0,
            player_pos.z - transform.translation.z,
        );
        let distance = to_player.length();

        let arrow_y = transform.translation.y;
        if distance <= ARROW_HIT_RADIUS
            && (0.0..=ARROW_HIT_HEIGHT).contains(&arrow_y)
            && !player_dodge.active
        {
            if player_health.apply_damage(ARROW_DAMAGE) > 0 {
                player_flash.trigger();
            }
            commands.entity(entity).despawn_recursive();
        }
    }
}
