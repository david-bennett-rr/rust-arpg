use std::f32::consts::PI;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::combat::{smoothstep01, DamageRng, FlashTint, HitFlash, HitPoints, StunMeter};
use crate::player::{Dodge, Player, PlayerCombat};
use crate::targeting::{HighlightGlow, TargetState, Targetable};
use crate::world::fog::FogDynamic;
use crate::world::tilemap::{sweep_ground_target_indexed, FloorBounds, WallSpatialIndex};

use super::{
    build_player_slash_state, try_player_slash, Dying, EnemyCollision, SlashTarget,
    UniqueEnemyMaterials,
};

#[derive(Component)]
pub struct DemonRat {
    home: Vec3,
    gait_phase: f32,
    move_blend: f32,
    attack_cooldown: f32,
    chomp: f32,
    recovery: f32,
    attack_hit_applied: bool,
    last_hit_swing_id: u32,
    alerted: bool,
}

#[derive(Component, Clone, Copy)]
pub(super) struct RatOwner(pub(super) Entity);

#[derive(Component, Clone, Copy)]
pub(super) struct RatRest {
    pub(super) translation: Vec3,
    pub(super) rotation: Quat,
}

impl RatRest {
    fn new(translation: Vec3, rotation: Quat) -> Self {
        Self {
            translation,
            rotation,
        }
    }
}

#[derive(Component, Clone, Copy)]
pub(super) enum RatJoint {
    Body,
    Head,
    Tail,
}

type RatPlayer<'w> = Single<
    'w,
    (
        &'static Transform,
        &'static PlayerCombat,
        &'static Dodge,
        &'static mut HitPoints,
        &'static mut HitFlash,
    ),
    (With<Player>, Without<DemonRat>),
>;

type RatActors<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut DemonRat,
        &'static mut Transform,
        &'static mut HitPoints,
        &'static mut StunMeter,
        &'static mut HitFlash,
    ),
    (Without<Player>, Without<Dying>),
>;

#[derive(SystemParam)]
pub(super) struct RatUpdateContext<'w, 's> {
    commands: Commands<'w, 's>,
    bounds: Res<'w, FloorBounds>,
    wall_index: Res<'w, WallSpatialIndex>,
    damage_rng: ResMut<'w, DamageRng>,
    target_state: ResMut<'w, TargetState>,
}

const RAT_MOVE_SPEED: f32 = 3.25;
const RAT_AGGRO_RANGE: f32 = 11.0;
const RAT_ATTACK_RANGE: f32 = 1.5;
const RAT_ATTACK_COOLDOWN: f32 = 0.9;
const RAT_LUNGE_SPEED: f32 = 4.35;
const RAT_ATTACK_DURATION: f32 = 0.48;
const RAT_HOP_HEIGHT: f32 = 0.42;
const RAT_BITE_DAMAGE: i32 = 3;
const RAT_BITE_HIT_PROGRESS: f32 = 0.58;
const RAT_BITE_REACH: f32 = 1.48;
const RAT_HP: i32 = 8;
const RAT_COLLISION_RADIUS: f32 = 0.58;
const RAT_RECOVERY_DURATION: f32 = 0.7;
const RAT_WALL_CLEARANCE: f32 = 0.04;

pub(super) fn do_spawn_rats(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spawn_positions: &[Vec3],
) {
    let fur_color = Vec3::new(0.24, 0.06, 0.07);
    let dark_fur_color = Vec3::new(0.10, 0.02, 0.03);
    let flesh_color = Vec3::new(0.48, 0.20, 0.17);
    let eye_color = Vec3::new(0.90, 0.18, 0.10);
    let bone_color = Vec3::new(0.78, 0.70, 0.60);

    let body_mesh = meshes.add(Sphere::new(0.36).mesh().ico(1).unwrap());
    let head_mesh = meshes.add(Sphere::new(0.22).mesh().ico(1).unwrap());
    let snout_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.11,
            radius_bottom: 0.06,
            height: 0.22,
        }
        .mesh()
        .resolution(5),
    );
    let ear_mesh = meshes.add(Cone::new(0.08, 0.16).mesh().resolution(4));
    let eye_mesh = meshes.add(Sphere::new(0.04).mesh().ico(1).unwrap());
    let tail_mesh = meshes.add(
        Capsule3d::new(0.035, 0.50)
            .mesh()
            .longitudes(5)
            .latitudes(4),
    );
    let claw_mesh = meshes.add(Cuboid::new(0.08, 0.10, 0.18));
    let spine_mesh = meshes.add(Cone::new(0.10, 0.28).mesh().resolution(4));

    let fur = materials.add(StandardMaterial {
        base_color: Color::srgb(fur_color.x, fur_color.y, fur_color.z),
        perceptual_roughness: 0.98,
        ..default()
    });
    let dark_fur = materials.add(StandardMaterial {
        base_color: Color::srgb(dark_fur_color.x, dark_fur_color.y, dark_fur_color.z),
        perceptual_roughness: 1.0,
        ..default()
    });
    let flesh = materials.add(StandardMaterial {
        base_color: Color::srgb(flesh_color.x, flesh_color.y, flesh_color.z),
        perceptual_roughness: 0.94,
        ..default()
    });
    let eye = materials.add(StandardMaterial {
        base_color: Color::srgb(eye_color.x, eye_color.y, eye_color.z),
        perceptual_roughness: 0.65,
        ..default()
    });
    let bone = materials.add(StandardMaterial {
        base_color: Color::srgb(bone_color.x, bone_color.y, bone_color.z),
        perceptual_roughness: 0.88,
        ..default()
    });

    for &home in spawn_positions {
        let rat = commands
            .spawn((
                DemonRat {
                    home,
                    gait_phase: home.x * 0.15 + home.z * 0.11,
                    move_blend: 0.0,
                    attack_cooldown: 0.25,
                    chomp: 0.0,
                    recovery: 0.0,
                    attack_hit_applied: false,
                    last_hit_swing_id: 0,
                    alerted: false,
                },
                EnemyCollision {
                    radius: RAT_COLLISION_RADIUS,
                    player_push_share: 0.15,
                },
                Targetable {
                    name: "Demon Rat".into(),
                    pick_radius: 1.0,
                },
                HighlightGlow::default(),
                HitPoints::new(RAT_HP),
                StunMeter::new(4.0, 2.5, 0.5),
                HitFlash::default(),
                FogDynamic::default(),
                UniqueEnemyMaterials,
                Transform::from_translation(home),
                Visibility::Hidden,
            ))
            .id();

        commands.entity(rat).with_children(|parent| {
            parent
                .spawn((
                    RatOwner(rat),
                    RatJoint::Body,
                    RatRest::new(Vec3::new(0.0, 0.28, 0.0), Quat::IDENTITY),
                    Transform::from_xyz(0.0, 0.28, 0.0),
                    Visibility::Inherited,
                ))
                .with_children(|body| {
                    body.spawn((
                        Mesh3d(body_mesh.clone()),
                        MeshMaterial3d(fur.clone()),
                        Transform::from_scale(Vec3::new(1.35, 0.78, 1.70)),
                        FlashTint {
                            owner: rat,
                            base_srgb: fur_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(spine_mesh.clone()),
                        MeshMaterial3d(bone.clone()),
                        Transform::from_xyz(0.0, 0.22, -0.02)
                            .with_rotation(Quat::from_rotation_x(-0.22))
                            .with_scale(Vec3::new(1.05, 1.25, 1.05)),
                        FlashTint {
                            owner: rat,
                            base_srgb: bone_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(spine_mesh.clone()),
                        MeshMaterial3d(bone.clone()),
                        Transform::from_xyz(0.0, 0.28, -0.28)
                            .with_rotation(Quat::from_rotation_x(-0.16))
                            .with_scale(Vec3::new(1.10, 1.45, 1.10)),
                        FlashTint {
                            owner: rat,
                            base_srgb: bone_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(spine_mesh.clone()),
                        MeshMaterial3d(bone.clone()),
                        Transform::from_xyz(0.0, 0.24, 0.18)
                            .with_rotation(Quat::from_rotation_x(-0.28))
                            .with_scale(Vec3::new(0.90, 1.10, 0.90)),
                        FlashTint {
                            owner: rat,
                            base_srgb: bone_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(claw_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(-0.18, -0.22, 0.30),
                        FlashTint {
                            owner: rat,
                            base_srgb: dark_fur_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(claw_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(0.18, -0.22, 0.30),
                        FlashTint {
                            owner: rat,
                            base_srgb: dark_fur_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(claw_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(-0.20, -0.22, -0.24),
                        FlashTint {
                            owner: rat,
                            base_srgb: dark_fur_color,
                        },
                    ));
                    body.spawn((
                        Mesh3d(claw_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(0.20, -0.22, -0.24),
                        FlashTint {
                            owner: rat,
                            base_srgb: dark_fur_color,
                        },
                    ));
                });

            parent
                .spawn((
                    RatOwner(rat),
                    RatJoint::Head,
                    RatRest::new(Vec3::new(0.0, 0.33, 0.48), Quat::from_rotation_x(0.08)),
                    Transform::from_xyz(0.0, 0.33, 0.48).with_rotation(Quat::from_rotation_x(0.08)),
                    Visibility::Inherited,
                ))
                .with_children(|head| {
                    head.spawn((
                        Mesh3d(head_mesh.clone()),
                        MeshMaterial3d(fur.clone()),
                        Transform::from_scale(Vec3::new(1.0, 0.84, 1.10)),
                        FlashTint {
                            owner: rat,
                            base_srgb: fur_color,
                        },
                    ));
                    head.spawn((
                        Mesh3d(snout_mesh.clone()),
                        MeshMaterial3d(flesh.clone()),
                        Transform::from_xyz(0.0, -0.02, 0.18)
                            .with_rotation(Quat::from_rotation_x(PI / 2.0)),
                        FlashTint {
                            owner: rat,
                            base_srgb: flesh_color,
                        },
                    ));
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(-0.12, 0.15, -0.02)
                            .with_rotation(Quat::from_rotation_x(-0.35)),
                        FlashTint {
                            owner: rat,
                            base_srgb: dark_fur_color,
                        },
                    ));
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(0.12, 0.15, -0.02)
                            .with_rotation(Quat::from_rotation_x(-0.35)),
                        FlashTint {
                            owner: rat,
                            base_srgb: dark_fur_color,
                        },
                    ));
                    head.spawn((
                        Mesh3d(eye_mesh.clone()),
                        MeshMaterial3d(eye.clone()),
                        Transform::from_xyz(-0.08, 0.03, 0.15),
                        FlashTint {
                            owner: rat,
                            base_srgb: eye_color,
                        },
                    ));
                    head.spawn((
                        Mesh3d(eye_mesh.clone()),
                        MeshMaterial3d(eye.clone()),
                        Transform::from_xyz(0.08, 0.03, 0.15),
                        FlashTint {
                            owner: rat,
                            base_srgb: eye_color,
                        },
                    ));
                });

            parent
                .spawn((
                    RatOwner(rat),
                    RatJoint::Tail,
                    RatRest::new(
                        Vec3::new(0.0, 0.24, -0.56),
                        Quat::from_rotation_x(-PI / 2.5),
                    ),
                    Transform::from_xyz(0.0, 0.24, -0.56)
                        .with_rotation(Quat::from_rotation_x(-PI / 2.5)),
                    Visibility::Inherited,
                ))
                .with_children(|tail| {
                    tail.spawn((
                        Mesh3d(tail_mesh.clone()),
                        MeshMaterial3d(flesh.clone()),
                        Transform::IDENTITY,
                        FlashTint {
                            owner: rat,
                            base_srgb: flesh_color,
                        },
                    ));
                });
        });
    }
}

pub(super) fn update_demon_rats(
    time: Res<Time>,
    mut ctx: RatUpdateContext<'_, '_>,
    mut player: RatPlayer<'_>,
    mut rats: RatActors<'_, '_>,
) {
    let delta = time.delta_secs();
    let (
        player_transform,
        player_combat,
        player_dodge,
        ref mut player_health,
        ref mut player_flash,
    ) = *player;
    let player_slash = build_player_slash_state(player_transform, player_combat);
    let player_ground = player_slash.ground;
    for (entity, mut rat, mut transform, ref mut rat_health, ref mut rat_stun, ref mut rat_flash) in
        &mut rats
    {
        rat.attack_cooldown = (rat.attack_cooldown - delta).max(0.0);
        let prev_chomp = rat.chomp;
        rat.chomp = (rat.chomp - delta / RAT_ATTACK_DURATION).max(0.0);
        // Start recovery when attack finishes
        if prev_chomp > 0.0 && rat.chomp <= 0.0 {
            rat.recovery = RAT_RECOVERY_DURATION;
        }
        rat.recovery = (rat.recovery - delta).max(0.0);
        transform.translation.y = 0.0;

        let rat_ground = Vec3::new(transform.translation.x, 0.0, transform.translation.z);
        let player_clear_path = ctx.wall_index.segment_clear(
            Vec2::new(player_ground.x, player_ground.z),
            Vec2::new(rat_ground.x, rat_ground.z),
            0.0,
        );
        if player_clear_path
            && try_player_slash(
                &mut ctx.commands,
                entity,
                rat_ground,
                SlashTarget {
                    last_hit_swing_id: &mut rat.last_hit_swing_id,
                    health: rat_health,
                    stun: rat_stun,
                    flash: rat_flash,
                },
                player_slash,
                &mut ctx.damage_rng,
                &mut ctx.target_state,
            )
        {
            continue;
        }

        // Stunned — skip all AI
        if rat_stun.stunned {
            rat.move_blend = 0.0;
            rat.chomp = 0.0;
            continue;
        }

        // Recovering after an attack — stay still
        if rat.recovery > 0.0 {
            rat.move_blend = 0.0;
            continue;
        }

        let to_player = player_ground - rat_ground;
        let player_distance = to_player.length();
        let to_home = rat.home - rat_ground;
        let home_distance = to_home.length();
        let clear_path_to_player = ctx.wall_index.segment_clear(
            Vec2::new(rat_ground.x, rat_ground.z),
            Vec2::new(player_ground.x, player_ground.z),
            RAT_COLLISION_RADIUS,
        );

        let chasing_player = player_distance <= RAT_AGGRO_RANGE && clear_path_to_player;
        rat.alerted = chasing_player;

        let mut move_direction = Vec3::ZERO;
        if chasing_player {
            if rat.chomp > 0.0 {
                if player_distance > 0.001 {
                    transform.look_to(-to_player.normalize_or_zero(), Vec3::Y);
                }
            } else if player_distance > RAT_ATTACK_RANGE {
                move_direction = to_player.normalize_or_zero();
            } else if rat.attack_cooldown <= 0.0 {
                rat.attack_cooldown = RAT_ATTACK_COOLDOWN;
                rat.chomp = 1.0;
                rat.attack_hit_applied = false;
            }
        } else if home_distance > 0.18 {
            move_direction = to_home.normalize_or_zero();
        }

        if move_direction.length_squared() > 0.0 {
            let move_speed = if chasing_player {
                RAT_MOVE_SPEED
            } else {
                RAT_MOVE_SPEED * 0.65
            };
            let step = move_speed * delta;
            let remaining = if chasing_player {
                (player_distance - RAT_ATTACK_RANGE).max(0.0)
            } else {
                home_distance
            };
            let travel = step.min(remaining);
            let current = Vec2::new(transform.translation.x, transform.translation.z);
            let desired_end = current + Vec2::new(move_direction.x, move_direction.z) * travel;
            let swept = sweep_ground_target_indexed(
                &ctx.bounds,
                current,
                desired_end,
                RAT_COLLISION_RADIUS,
                &ctx.wall_index,
                RAT_WALL_CLEARANCE,
            );
            let moved_distance = swept.distance(current);
            transform.translation.x = swept.x;
            transform.translation.z = swept.y;

            if moved_distance > 0.0001 {
                transform.look_to(-move_direction, Vec3::Y);
                rat.gait_phase += delta * 12.0;
                rat.gait_phase %= 100.0 * std::f32::consts::TAU;
                rat.move_blend = 1.0;
            } else {
                rat.move_blend = 0.0;
            }
        } else {
            if chasing_player && player_distance > 0.001 {
                transform.look_to(-to_player.normalize_or_zero(), Vec3::Y);
            }
            rat.move_blend = 0.0;
        }

        if let Some(attack_progress) = rat_attack_progress(rat.chomp) {
            let hop = rat_hop_arc(attack_progress);
            let lunge = (hop * delta * RAT_LUNGE_SPEED).min(0.18);
            let forward = transform.rotation * Vec3::Z;
            transform.translation.y = hop * RAT_HOP_HEIGHT;
            let current = Vec2::new(transform.translation.x, transform.translation.z);
            let desired_end = current + Vec2::new(forward.x, forward.z) * lunge;
            let swept = sweep_ground_target_indexed(
                &ctx.bounds,
                current,
                desired_end,
                RAT_COLLISION_RADIUS,
                &ctx.wall_index,
                RAT_WALL_CLEARANCE,
            );
            transform.translation.x = swept.x;
            transform.translation.z = swept.y;
            let bite_distance = super::horizontal_distance(player_ground, transform.translation);

            if !rat.attack_hit_applied
                && attack_progress >= RAT_BITE_HIT_PROGRESS
                && bite_distance <= RAT_BITE_REACH
                && !player_dodge.active
            {
                rat.attack_hit_applied = true;
                if player_health.apply_damage(RAT_BITE_DAMAGE) > 0 {
                    player_flash.trigger();
                }
            }
        }
    }
}

pub(super) fn animate_demon_rats(
    time: Res<Time>,
    rats: Query<&DemonRat, Without<Dying>>,
    mut joints: Query<(&RatOwner, &RatJoint, &RatRest, &mut Transform)>,
) {
    for (owner, joint, rest, mut transform) in &mut joints {
        let Ok(rat) = rats.get(owner.0) else {
            continue;
        };

        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        let t = time.elapsed_secs();
        let move_wave = rat.gait_phase.sin() * rat.move_blend;
        let move_bob = (rat.gait_phase * 2.0).sin() * 0.04 * rat.move_blend;
        let attack_progress = rat_attack_progress(rat.chomp);
        let hop = attack_progress.map(rat_hop_arc).unwrap_or(0.0);
        let bite = attack_progress.map(rat_nibble_curve).unwrap_or(0.0);
        let alert = if rat.alerted { 1.0 } else { 0.0 };

        // Idle fidgeting (when not moving or attacking)
        let idle = rat.move_blend == 0.0 && rat.chomp <= 0.0;
        let sniff = if idle {
            // Quick periodic sniffing motion
            let sniff_cycle = (t * 3.5 + rat.home.x * 1.7).sin();
            (sniff_cycle * 0.5 + 0.5).powi(4)
        } else {
            0.0
        };
        let idle_look = if idle {
            (t * 0.7 + rat.home.z * 0.9).sin()
        } else {
            0.0
        };
        let idle_breath = if idle {
            (t * 1.8 + rat.home.x * 0.5).sin()
        } else {
            0.0
        };

        match joint {
            RatJoint::Body => {
                transform.translation.y += move_bob + hop * 0.08 + idle_breath * 0.008;
                transform.rotation *= Quat::from_rotation_x(
                    -bite * 0.16 - hop * 0.06 + idle_breath * 0.012 + sniff * 0.03,
                ) * Quat::from_rotation_z(move_wave * 0.06)
                    * Quat::from_rotation_y(idle_look * 0.04);
            }
            RatJoint::Head => {
                transform.translation.y += move_bob * 0.6 + hop * 0.05;
                transform.rotation *=
                    Quat::from_rotation_x(bite * 0.70 - alert * 0.05 + sniff * 0.14)
                        * Quat::from_rotation_y(move_wave * 0.12 + idle_look * 0.22);
            }
            RatJoint::Tail => {
                let tail_sway = (rat.gait_phase * 0.75 + t + rat.home.x * 0.06).sin();
                let idle_tail_flick = if idle {
                    let flick = (t * 1.2 + rat.home.z * 2.3).sin();
                    let burst = (flick * 0.5 + 0.5).powi(6);
                    burst * 0.35
                } else {
                    0.0
                };
                transform.rotation *= Quat::from_rotation_y(
                    tail_sway * (0.24 + rat.move_blend * 0.22) + idle_tail_flick,
                ) * Quat::from_rotation_x(bite * 0.10 - hop * 0.12);
            }
        }
    }
}

fn rat_attack_progress(chomp: f32) -> Option<f32> {
    if chomp <= 0.0 {
        None
    } else {
        Some(1.0 - chomp.clamp(0.0, 1.0))
    }
}

fn rat_hop_arc(progress: f32) -> f32 {
    (progress * PI).sin().max(0.0)
}

fn rat_nibble_curve(progress: f32) -> f32 {
    let window = 1.0 - ((progress - 0.64).abs() / 0.24).clamp(0.0, 1.0);
    smoothstep01(window)
}
