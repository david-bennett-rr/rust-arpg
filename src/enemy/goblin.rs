use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{DamageRng, FlashTint, HitFlash, HitPoints, StunMeter, smoothstep01};
use crate::player::{Player, PlayerCombat};
use crate::targeting::{HighlightGlow, TargetState, Targetable};
use crate::world::tilemap::{clamp_translation_to_arena, grid_to_world};

use super::{
    EnemyCollision, SlashTarget, UniqueEnemyMaterials, build_player_slash_state,
    try_player_slash,
};
use super::arrow::{spawn_arrow, ArrowMeshes};

#[derive(Component)]
pub struct GoblinArcher {
    home: Vec3,
    gait_phase: f32,
    move_blend: f32,
    shoot_cooldown: f32,
    draw_timer: f32,
    last_hit_swing_id: u32,
    alerted: bool,
}

type GoblinPlayer<'w> =
    Single<'w, (&'static Transform, &'static PlayerCombat), (With<Player>, Without<GoblinArcher>)>;

type GoblinActors<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut GoblinArcher,
        &'static mut Transform,
        &'static mut HitPoints,
        &'static mut StunMeter,
        &'static mut HitFlash,
    ),
    Without<Player>,
>;

const GOBLIN_HP: i32 = 6;
const GOBLIN_MOVE_SPEED: f32 = 2.0;
const GOBLIN_AGGRO_RANGE: f32 = 14.0;
const GOBLIN_PREFERRED_RANGE: f32 = 8.0;
const GOBLIN_TOO_CLOSE: f32 = 4.5;
const GOBLIN_SHOOT_COOLDOWN: f32 = 2.2;
const GOBLIN_DRAW_TIME: f32 = 0.7;
const GOBLIN_COLLISION_RADIUS: f32 = 0.75;

pub(super) const GOBLIN_SPAWN_POINTS: [(i32, i32); 3] = [
    (4, 5),
    (16, 7),
    (14, 15),
];

pub(super) fn spawn_goblin_archers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    do_spawn_goblins(&mut commands, &mut meshes, &mut materials);
}

pub(super) fn do_spawn_goblins(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let skin_color = Vec3::new(0.30, 0.42, 0.18);
    let dark_skin_color = Vec3::new(0.20, 0.30, 0.12);
    let cloth_color = Vec3::new(0.28, 0.22, 0.14);
    let wood_color = Vec3::new(0.40, 0.28, 0.14);
    let eye_color = Vec3::new(0.85, 0.72, 0.10);

    let body_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.28,
            radius_bottom: 0.38,
            height: 0.90,
        }
        .mesh()
        .resolution(6),
    );
    let head_mesh = meshes.add(Sphere::new(0.34).mesh().ico(1).unwrap());
    let eye_mesh = meshes.add(Sphere::new(0.07).mesh().ico(1).unwrap());
    let ear_mesh = meshes.add(Cone::new(0.12, 0.34).mesh().resolution(4));
    let nose_mesh = meshes.add(Cone::new(0.07, 0.18).mesh().resolution(4));
    let arm_mesh = meshes.add(Capsule3d::new(0.10, 0.50).mesh().longitudes(5).latitudes(4));
    let leg_mesh = meshes.add(Capsule3d::new(0.11, 0.52).mesh().longitudes(5).latitudes(4));
    // Bow pieces: upper limb, lower limb, grip, string
    let bow_limb_mesh = meshes.add(
        Capsule3d::new(0.025, 0.38)
            .mesh()
            .longitudes(5)
            .latitudes(4),
    );
    let bow_grip_mesh = meshes.add(Cylinder::new(0.035, 0.14).mesh().resolution(5));
    let bow_string_mesh = meshes.add(Cylinder::new(0.008, 0.80).mesh().resolution(4));
    let hat_mesh = meshes.add(Cone::new(0.28, 0.48).mesh().resolution(5));

    let skin = materials.add(StandardMaterial {
        base_color: Color::srgb(skin_color.x, skin_color.y, skin_color.z),
        perceptual_roughness: 0.95,
        ..default()
    });
    let dark_skin = materials.add(StandardMaterial {
        base_color: Color::srgb(dark_skin_color.x, dark_skin_color.y, dark_skin_color.z),
        perceptual_roughness: 0.98,
        ..default()
    });
    let cloth = materials.add(StandardMaterial {
        base_color: Color::srgb(cloth_color.x, cloth_color.y, cloth_color.z),
        perceptual_roughness: 1.0,
        ..default()
    });
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(wood_color.x, wood_color.y, wood_color.z),
        perceptual_roughness: 0.90,
        ..default()
    });
    let eye_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(eye_color.x, eye_color.y, eye_color.z),
        perceptual_roughness: 0.65,
        ..default()
    });

    for &(gx, gz) in &GOBLIN_SPAWN_POINTS {
        let home = grid_to_world(gx, gz);
        let goblin = commands
            .spawn((
                GoblinArcher {
                    home,
                    gait_phase: home.x * 0.2 + home.z * 0.13,
                    move_blend: 0.0,
                    shoot_cooldown: 1.0,
                    draw_timer: 0.0,
                    last_hit_swing_id: 0,
                    alerted: false,
                },
                EnemyCollision {
                    radius: GOBLIN_COLLISION_RADIUS,
                    player_push_share: 0.4,
                },
                Targetable {
                    name: "Goblin Archer".into(),
                    pick_radius: 1.2,
                },
                HighlightGlow::default(),
                HitPoints::new(GOBLIN_HP),
                StunMeter::new(3.0, 3.0, 1.2),
                HitFlash::default(),
                UniqueEnemyMaterials,
                Transform::from_translation(home),
                Visibility::Visible,
            ))
            .id();

        commands.entity(goblin).with_children(|parent| {
            // Body (torso)
            parent.spawn((
                Mesh3d(body_mesh.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.0, 1.05, 0.0),
                FlashTint {
                    owner: goblin,
                    base_srgb: cloth_color,
                },
            ));

            // Head
            parent
                .spawn((Transform::from_xyz(0.0, 1.78, 0.0), Visibility::Inherited))
                .with_children(|head| {
                    head.spawn((
                        Mesh3d(head_mesh.clone()),
                        MeshMaterial3d(skin.clone()),
                        Transform::IDENTITY,
                        FlashTint {
                            owner: goblin,
                            base_srgb: skin_color,
                        },
                    ));
                    // Eyes
                    head.spawn((
                        Mesh3d(eye_mesh.clone()),
                        MeshMaterial3d(eye_mat.clone()),
                        Transform::from_xyz(-0.13, 0.05, 0.28),
                        FlashTint { owner: goblin, base_srgb: eye_color },
                    ));
                    head.spawn((
                        Mesh3d(eye_mesh.clone()),
                        MeshMaterial3d(eye_mat.clone()),
                        Transform::from_xyz(0.13, 0.05, 0.28),
                        FlashTint { owner: goblin, base_srgb: eye_color },
                    ));
                    // Pointy ears
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::from_xyz(-0.34, 0.06, 0.0)
                            .with_rotation(Quat::from_rotation_z(PI / 2.5)),
                        FlashTint { owner: goblin, base_srgb: dark_skin_color },
                    ));
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::from_xyz(0.34, 0.06, 0.0)
                            .with_rotation(Quat::from_rotation_z(-PI / 2.5)),
                        FlashTint { owner: goblin, base_srgb: dark_skin_color },
                    ));
                    // Nose
                    head.spawn((
                        Mesh3d(nose_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::from_xyz(0.0, -0.04, 0.30)
                            .with_rotation(Quat::from_rotation_x(PI / 2.0)),
                        FlashTint { owner: goblin, base_srgb: dark_skin_color },
                    ));
                    // Pointy hat
                    head.spawn((
                        Mesh3d(hat_mesh.clone()),
                        MeshMaterial3d(cloth.clone()),
                        Transform::from_xyz(0.0, 0.40, 0.0)
                            .with_rotation(Quat::from_rotation_z(0.15)),
                        FlashTint { owner: goblin, base_srgb: cloth_color },
                    ));
                });

            // Left arm (bow arm)
            parent.spawn((
                Mesh3d(arm_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(-0.42, 1.20, 0.14)
                    .with_rotation(Quat::from_rotation_x(-0.4) * Quat::from_rotation_z(0.2)),
                FlashTint { owner: goblin, base_srgb: skin_color },
            ));
            // Right arm
            parent.spawn((
                Mesh3d(arm_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.42, 1.20, 0.0).with_rotation(Quat::from_rotation_z(-0.15)),
                FlashTint { owner: goblin, base_srgb: skin_color },
            ));

            // Legs
            parent.spawn((
                Mesh3d(leg_mesh.clone()),
                MeshMaterial3d(dark_skin.clone()),
                Transform::from_xyz(-0.15, 0.42, 0.0),
                FlashTint { owner: goblin, base_srgb: dark_skin_color },
            ));
            parent.spawn((
                Mesh3d(leg_mesh.clone()),
                MeshMaterial3d(dark_skin.clone()),
                Transform::from_xyz(0.15, 0.42, 0.0),
                FlashTint { owner: goblin, base_srgb: dark_skin_color },
            ));

            // Bow
            parent
                .spawn((
                    Transform::from_xyz(-0.46, 1.22, 0.20)
                        .with_rotation(Quat::from_rotation_x(-0.3)),
                    Visibility::Inherited,
                ))
                .with_children(|bow| {
                    // Grip (center)
                    bow.spawn((
                        Mesh3d(bow_grip_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::IDENTITY,
                        FlashTint { owner: goblin, base_srgb: dark_skin_color },
                    ));
                    // Upper limb (angled outward)
                    bow.spawn((
                        Mesh3d(bow_limb_mesh.clone()),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_xyz(0.0, 0.26, -0.06)
                            .with_rotation(Quat::from_rotation_x(0.25)),
                        FlashTint { owner: goblin, base_srgb: wood_color },
                    ));
                    // Lower limb (angled outward)
                    bow.spawn((
                        Mesh3d(bow_limb_mesh.clone()),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_xyz(0.0, -0.26, -0.06)
                            .with_rotation(Quat::from_rotation_x(-0.25)),
                        FlashTint { owner: goblin, base_srgb: wood_color },
                    ));
                    // String (taut, straight between limb tips)
                    bow.spawn((
                        Mesh3d(bow_string_mesh.clone()),
                        MeshMaterial3d(cloth.clone()),
                        Transform::from_xyz(0.0, 0.0, 0.05),
                        FlashTint { owner: goblin, base_srgb: cloth_color },
                    ));
                });
        });
    }
}

pub(super) fn update_goblin_archers(
    mut commands: Commands,
    time: Res<Time>,
    player: GoblinPlayer<'_>,
    mut goblins: GoblinActors<'_, '_>,
    arrow_meshes: Res<ArrowMeshes>,
    mut damage_rng: ResMut<DamageRng>,
    mut target_state: ResMut<TargetState>,
) {
    let delta = time.delta_secs();
    let (player_transform, player_combat) = *player;
    let player_slash = build_player_slash_state(player_transform, player_combat);
    let player_ground = player_slash.ground;

    for (
        entity,
        mut goblin,
        mut transform,
        ref mut goblin_health,
        ref mut goblin_stun,
        ref mut goblin_flash,
    ) in &mut goblins
    {
        goblin.shoot_cooldown = (goblin.shoot_cooldown - delta).max(0.0);
        transform.translation.y = 0.0;

        let goblin_ground = Vec3::new(transform.translation.x, 0.0, transform.translation.z);

        // Take damage from player sword
        if try_player_slash(
            &mut commands,
            entity,
            goblin_ground,
            SlashTarget {
                last_hit_swing_id: &mut goblin.last_hit_swing_id,
                health: goblin_health,
                stun: goblin_stun,
                flash: goblin_flash,
            },
            player_slash,
            &mut damage_rng,
            &mut target_state,
        ) {
            continue;
        }

        // Stunned — skip all AI
        if goblin_stun.stunned {
            goblin.move_blend = 0.0;
            goblin.draw_timer = 0.0;
            continue;
        }

        let to_player = player_ground - goblin_ground;
        let player_distance = to_player.length();
        let to_home = goblin.home - goblin_ground;
        let home_distance = to_home.length();

        let chasing_player = player_distance <= GOBLIN_AGGRO_RANGE;
        goblin.alerted = chasing_player;

        // Drawing bow — stand still, face player, fire when ready
        let drawing = goblin.draw_timer > 0.0;
        if drawing {
            goblin.draw_timer -= delta;
            if player_distance > 0.001 {
                transform.look_to(-to_player.normalize_or_zero(), Vec3::Y);
            }
            goblin.move_blend = 0.0;

            if goblin.draw_timer <= 0.0 {
                // Fire!
                goblin.draw_timer = 0.0;
                let shoot_dir = to_player.normalize_or_zero();
                let arrow_start = goblin_ground + Vec3::Y * 1.2 + shoot_dir * 0.5;
                spawn_arrow(&mut commands, &arrow_meshes, arrow_start, shoot_dir);
            }
        } else {
            let mut move_direction = Vec3::ZERO;
            let mut fleeing = false;
            if chasing_player {
                if player_distance < GOBLIN_TOO_CLOSE {
                    // Run away
                    move_direction = -to_player.normalize_or_zero();
                    fleeing = true;
                } else if player_distance > GOBLIN_PREFERRED_RANGE + 1.0 {
                    // Move closer
                    move_direction = to_player.normalize_or_zero();
                } else if goblin.shoot_cooldown <= 0.0 {
                    // In sweet spot — start drawing
                    goblin.shoot_cooldown = GOBLIN_SHOOT_COOLDOWN;
                    goblin.draw_timer = GOBLIN_DRAW_TIME;
                }

                // Face player when standing still or approaching, not when fleeing
                if !fleeing && player_distance > 0.001 {
                    transform.look_to(-to_player.normalize_or_zero(), Vec3::Y);
                }
            } else if home_distance > 0.18 {
                move_direction = to_home.normalize_or_zero();
            }

            if move_direction.length_squared() > 0.0 {
                let speed = if fleeing {
                    GOBLIN_MOVE_SPEED * 1.3
                } else {
                    GOBLIN_MOVE_SPEED
                };
                let step = speed * delta;
                let travel = if chasing_player {
                    step
                } else {
                    step.min(home_distance)
                };
                transform.translation += move_direction * travel;
                clamp_translation_to_arena(&mut transform.translation, GOBLIN_COLLISION_RADIUS);
                // Face movement direction (turns around when fleeing)
                transform.look_to(-move_direction, Vec3::Y);
                goblin.gait_phase += delta * 10.0;
                goblin.gait_phase %= 100.0 * std::f32::consts::TAU;
                goblin.move_blend = 1.0;
            } else {
                goblin.move_blend = 0.0;
            }
        }
    }
}

pub(super) fn animate_goblin_archers(time: Res<Time>, mut goblins: Query<(&GoblinArcher, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (goblin, mut transform) in &mut goblins {
        // Walking bob
        if goblin.move_blend > 0.0 {
            let bob = (goblin.gait_phase * 2.0).sin().abs() * 0.06;
            transform.translation.y = bob;
        }

        // Draw bow lean
        if goblin.draw_timer > 0.0 {
            let draw_progress = 1.0 - (goblin.draw_timer / GOBLIN_DRAW_TIME);
            transform.rotation *= Quat::from_rotation_x(smoothstep01(draw_progress) * 0.12);
        }

        // Idle sway when not alerted and standing still
        if !goblin.alerted && goblin.move_blend == 0.0 {
            let sway = (t * 0.8 + goblin.home.x * 0.3).sin() * 0.02;
            transform.translation.y += sway.abs() * 0.03;
            transform.rotation *= Quat::from_rotation_z(sway);
        }
    }
}
