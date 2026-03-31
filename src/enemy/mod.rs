use std::{
    f32::consts::PI,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;

use crate::combat::{FlashTint, HitFlash, HitPoints, StunMeter, game_running};
use crate::player::{Dodge, Player, PlayerCombat, PlayerSet, visual_forward};
use crate::targeting::{HighlightGlow, Targetable};
use crate::world::tilemap::{MAP_SIZE, TILE_SIZE, grid_to_world};

#[derive(Event)]
pub struct RespawnEnemies;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RespawnEnemies>()
            .add_systems(
                Startup,
                (spawn_demon_rats, setup_arrow_meshes, spawn_goblin_archers).chain(),
            )
            .add_systems(
            Update,
            (
                update_demon_rats,
                update_goblin_archers,
                update_arrows,
                animate_demon_rats,
                resolve_actor_collisions,
            )
                .chain()
                .run_if(game_running)
                .after(PlayerSet::Update),
        )
        .add_systems(
            Update,
            handle_respawn_enemies,
        );
    }
}

#[derive(Component)]
pub struct DemonRat {
    home: Vec3,
    gait_phase: f32,
    move_blend: f32,
    attack_cooldown: f32,
    chomp: f32,
    attack_hit_applied: bool,
    last_hit_swing_id: u32,
    alerted: bool,
}

#[derive(Component, Clone, Copy)]
struct EnemyCollision {
    radius: f32,
    player_push_ratio: f32,
}

#[derive(Component, Clone, Copy)]
struct RatOwner(Entity);

#[derive(Component, Clone, Copy)]
struct RatRest {
    translation: Vec3,
    rotation: Quat,
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
enum RatJoint {
    Body,
    Head,
    Tail,
}

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

#[derive(Component)]
pub struct Arrow {
    direction: Vec3,
    lifetime: f32,
}

const RAT_MOVE_SPEED: f32 = 3.25;
const RAT_AGGRO_RANGE: f32 = 11.0;
const RAT_ATTACK_RANGE: f32 = 1.5;
const RAT_ATTACK_COOLDOWN: f32 = 0.9;
const RAT_LUNGE_SPEED: f32 = 4.35;
const RAT_ATTACK_DURATION: f32 = 0.48;
const RAT_HOP_HEIGHT: f32 = 0.42;
const RAT_BITE_DAMAGE: i32 = 1;
const RAT_BITE_HIT_PROGRESS: f32 = 0.58;
const RAT_BITE_REACH: f32 = 1.48;
const RAT_HP: i32 = 8;
const PLAYER_SLASH_RANGE: f32 = 2.5;
const PLAYER_SLASH_ARC_DOT: f32 = 0.08;
const PLAYER_COLLISION_RADIUS: f32 = 0.78;
const RAT_COLLISION_RADIUS: f32 = 0.58;
const COLLISION_EPSILON: f32 = 0.001;

// Goblin Archer
const GOBLIN_HP: i32 = 6;
const GOBLIN_MOVE_SPEED: f32 = 2.0;
const GOBLIN_AGGRO_RANGE: f32 = 14.0;
const GOBLIN_PREFERRED_RANGE: f32 = 8.0;
const GOBLIN_TOO_CLOSE: f32 = 4.5;
const GOBLIN_SHOOT_COOLDOWN: f32 = 2.2;
const GOBLIN_DRAW_TIME: f32 = 0.7;
const GOBLIN_COLLISION_RADIUS: f32 = 0.75;

// Arrow
const ARROW_SPEED: f32 = 6.0;
const ARROW_DAMAGE: i32 = 2;
const ARROW_LIFETIME: f32 = 4.0;
const ARROW_HIT_RADIUS: f32 = 0.5;

fn spawn_demon_rats(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    do_spawn_rats(&mut commands, &mut meshes, &mut materials);
}

fn do_spawn_rats(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
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

    let spawn_points = [
        grid_to_world(6, 8),
        grid_to_world(8, 14),
        grid_to_world(13, 6),
        grid_to_world(15, 11),
        grid_to_world(12, 15),
        grid_to_world(4, 11),
    ];

    for home in spawn_points {
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

        let rat = commands
            .spawn((
                DemonRat {
                    home,
                    gait_phase: home.x * 0.15 + home.z * 0.11,
                    move_blend: 0.0,
                    attack_cooldown: 0.25,
                    chomp: 0.0,
                    attack_hit_applied: false,
                    last_hit_swing_id: 0,
                    alerted: false,
                },
                EnemyCollision {
                    radius: RAT_COLLISION_RADIUS,
                    player_push_ratio: 0.0,
                },
                Targetable {
                    name: "Demon Rat".into(),
                    pick_radius: 1.0,
                },
                HighlightGlow::default(),
                HitPoints::new(RAT_HP),
                StunMeter::new(4.0, 2.5, 1.5),
                HitFlash::default(),
                Transform::from_translation(home),
                Visibility::Visible,
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
                    ));
                    body.spawn((
                        Mesh3d(spine_mesh.clone()),
                        MeshMaterial3d(bone.clone()),
                        Transform::from_xyz(0.0, 0.24, 0.18)
                            .with_rotation(Quat::from_rotation_x(-0.28))
                            .with_scale(Vec3::new(0.90, 1.10, 0.90)),
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
                    ));
                    body.spawn((
                        Mesh3d(claw_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(-0.20, -0.22, -0.24),
                    ));
                    body.spawn((
                        Mesh3d(claw_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(0.20, -0.22, -0.24),
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
                    ));
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_fur.clone()),
                        Transform::from_xyz(0.12, 0.15, -0.02)
                            .with_rotation(Quat::from_rotation_x(-0.35)),
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
                    ));
                });
        });
    }
}

fn update_demon_rats(
    mut commands: Commands,
    time: Res<Time>,
    mut player: Single<
        (&Transform, &PlayerCombat, &Dodge, &mut HitPoints, &mut HitFlash),
        (With<Player>, Without<DemonRat>),
    >,
    mut rats: Query<
        (
            Entity,
            &mut DemonRat,
            &mut Transform,
            &mut HitPoints,
            &mut StunMeter,
            &mut HitFlash,
        ),
        Without<Player>,
    >,
    mut damage_rng: Local<u64>,
) {
    let delta = time.delta_secs();
    let (player_transform, player_combat, player_dodge, ref mut player_health, ref mut player_flash) = *player;
    let player_ground = Vec3::new(
        player_transform.translation.x,
        0.0,
        player_transform.translation.z,
    );
    let player_forward = visual_forward(player_transform).normalize_or_zero();
    let slash_ready = player_combat.strike > 0.42;

    for (entity, mut rat, mut transform, ref mut rat_health, ref mut rat_stun, ref mut rat_flash) in &mut rats {
        rat.attack_cooldown = (rat.attack_cooldown - delta).max(0.0);
        rat.chomp = (rat.chomp - delta / RAT_ATTACK_DURATION).max(0.0);
        transform.translation.y = 0.0;

        let rat_ground = Vec3::new(transform.translation.x, 0.0, transform.translation.z);
        let to_rat = rat_ground - player_ground;
        let rat_distance = to_rat.length();
        if slash_ready && player_combat.swing_id != rat.last_hit_swing_id {
            let rat_direction = to_rat.normalize_or_zero();
            if rat_distance <= PLAYER_SLASH_RANGE
                && rat_direction.dot(player_forward) >= PLAYER_SLASH_ARC_DOT
            {
                rat.last_hit_swing_id = player_combat.swing_id;
                let damage = roll_1d5(&mut damage_rng);
                if rat_health.apply_damage(damage) > 0 {
                    rat_stun.apply_stun_damage(damage as f32);
                    rat_flash.trigger();
                    if rat_health.is_dead() {
                        commands.entity(entity).despawn_recursive();
                        continue;
                    }
                }
            }
        }

        // Stunned — skip all AI
        if rat_stun.stunned {
            rat.move_blend = 0.0;
            rat.chomp = 0.0;
            continue;
        }

        let to_player = player_ground - rat_ground;
        let player_distance = to_player.length();
        let to_home = rat.home - rat_ground;
        let home_distance = to_home.length();

        let chasing_player = player_distance <= RAT_AGGRO_RANGE;
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
            transform.translation += move_direction * travel;
            transform.look_to(-move_direction, Vec3::Y);
            rat.gait_phase += delta * 12.0;
            rat.move_blend = 1.0;
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
            transform.translation += forward * lunge;

            if !rat.attack_hit_applied
                && attack_progress >= RAT_BITE_HIT_PROGRESS
                && player_distance <= RAT_BITE_REACH
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

fn animate_demon_rats(
    time: Res<Time>,
    rats: Query<&DemonRat>,
    mut joints: Query<(&RatOwner, &RatJoint, &RatRest, &mut Transform)>,
) {
    for (owner, joint, rest, mut transform) in &mut joints {
        let Ok(rat) = rats.get(owner.0) else {
            continue;
        };

        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        let move_wave = rat.gait_phase.sin() * rat.move_blend;
        let move_bob = (rat.gait_phase * 2.0).sin() * 0.04 * rat.move_blend;
        let attack_progress = rat_attack_progress(rat.chomp);
        let hop = attack_progress.map(rat_hop_arc).unwrap_or(0.0);
        let bite = attack_progress.map(rat_nibble_curve).unwrap_or(0.0);
        let alert = if rat.alerted { 1.0 } else { 0.0 };

        match joint {
            RatJoint::Body => {
                transform.translation.y += move_bob + hop * 0.08;
                transform.rotation *= Quat::from_rotation_x(-bite * 0.16 - hop * 0.06)
                    * Quat::from_rotation_z(move_wave * 0.06);
            }
            RatJoint::Head => {
                transform.translation.y += move_bob * 0.6 + hop * 0.05;
                transform.rotation *= Quat::from_rotation_x(bite * 0.70 - alert * 0.05)
                    * Quat::from_rotation_y(move_wave * 0.12);
            }
            RatJoint::Tail => {
                let tail_sway =
                    (rat.gait_phase * 0.75 + time.elapsed_secs() + rat.home.x * 0.06).sin();
                transform.rotation *=
                    Quat::from_rotation_y(tail_sway * (0.24 + rat.move_blend * 0.22))
                        * Quat::from_rotation_x(bite * 0.10 - hop * 0.12);
            }
        }
    }
}

fn resolve_actor_collisions(
    mut player: Single<(&mut Transform, &Dodge), (With<Player>, Without<EnemyCollision>)>,
    enemy_entities: Query<Entity, With<EnemyCollision>>,
    mut enemy_transforms: Query<
        (&EnemyCollision, &mut Transform),
        (With<EnemyCollision>, Without<Player>),
    >,
) {
    let (ref mut player_tf, dodge) = *player;
    let mut player_ground = horizontal_position(player_tf.translation);
    let enemy_ids: Vec<Entity> = enemy_entities.iter().collect();

    // Skip player-enemy collision while dodging
    if !dodge.active {
        for enemy_id in &enemy_ids {
            let Ok((collision, mut enemy_transform)) = enemy_transforms.get_mut(*enemy_id) else {
                continue;
            };
            let mut enemy_ground = horizontal_position(enemy_transform.translation);
            let separation = resolve_overlap(
                &mut player_ground,
                &mut enemy_ground,
                PLAYER_COLLISION_RADIUS + collision.radius,
                collision.player_push_ratio,
            );

            if separation {
                player_tf.translation.x = player_ground.x;
                player_tf.translation.z = player_ground.y;
                enemy_transform.translation.x = enemy_ground.x;
                enemy_transform.translation.z = enemy_ground.y;
                clamp_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
                clamp_to_arena(&mut enemy_transform.translation, collision.radius);
                player_ground = horizontal_position(player_tf.translation);
            }
        }
    }

    for i in 0..enemy_ids.len() {
        for j in (i + 1)..enemy_ids.len() {
            let Ok([(collision_a, mut enemy_a), (collision_b, mut enemy_b)]) =
                enemy_transforms.get_many_mut([enemy_ids[i], enemy_ids[j]])
            else {
                continue;
            };
            let mut a_ground = horizontal_position(enemy_a.translation);
            let mut b_ground = horizontal_position(enemy_b.translation);
            let separated = resolve_overlap(
                &mut a_ground,
                &mut b_ground,
                collision_a.radius + collision_b.radius,
                0.5,
            );

            if separated {
                enemy_a.translation.x = a_ground.x;
                enemy_a.translation.z = a_ground.y;
                enemy_b.translation.x = b_ground.x;
                enemy_b.translation.z = b_ground.y;
                clamp_to_arena(&mut enemy_a.translation, collision_a.radius);
                clamp_to_arena(&mut enemy_b.translation, collision_b.radius);
            }
        }
    }
}

fn smoothstep01(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
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

fn horizontal_position(translation: Vec3) -> Vec2 {
    Vec2::new(translation.x, translation.z)
}

fn resolve_overlap(a: &mut Vec2, b: &mut Vec2, minimum_distance: f32, a_push_ratio: f32) -> bool {
    let offset = *b - *a;
    let distance = offset.length();
    let penetration = minimum_distance - distance;
    if penetration <= 0.0 {
        return false;
    }

    let direction = if distance > COLLISION_EPSILON {
        offset / distance
    } else {
        Vec2::X
    };
    let a_push = penetration * a_push_ratio;
    let b_push = penetration - a_push;

    *a -= direction * a_push;
    *b += direction * b_push;
    true
}

fn clamp_to_arena(translation: &mut Vec3, radius: f32) {
    let half_extent = (MAP_SIZE as f32 - 1.0) * TILE_SIZE * 0.5 - radius - 0.05;
    translation.x = translation.x.clamp(-half_extent, half_extent);
    translation.z = translation.z.clamp(-half_extent, half_extent);
}

// ---------------------------------------------------------------------------
// Goblin Archers
// ---------------------------------------------------------------------------

fn spawn_goblin_archers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    do_spawn_goblins(&mut commands, &mut meshes, &mut materials);
}

fn do_spawn_goblins(
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
    let bow_limb_mesh = meshes.add(Capsule3d::new(0.025, 0.38).mesh().longitudes(5).latitudes(4));
    let bow_grip_mesh = meshes.add(Cylinder::new(0.035, 0.14).mesh().resolution(5));
    let bow_string_mesh = meshes.add(Cylinder::new(0.008, 0.80).mesh().resolution(4));
    let hat_mesh = meshes.add(Cone::new(0.28, 0.48).mesh().resolution(5));

    let spawn_points = [
        grid_to_world(4, 5),
        grid_to_world(16, 7),
        grid_to_world(14, 15),
    ];

    for home in spawn_points {
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
                    player_push_ratio: 1.0,
                },
                Targetable {
                    name: "Goblin Archer".into(),
                    pick_radius: 1.2,
                },
                HighlightGlow::default(),
                HitPoints::new(GOBLIN_HP),
                StunMeter::new(3.0, 3.0, 1.2),
                HitFlash::default(),
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
                .spawn((
                    Transform::from_xyz(0.0, 1.78, 0.0),
                    Visibility::Inherited,
                ))
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
                    ));
                    head.spawn((
                        Mesh3d(eye_mesh.clone()),
                        MeshMaterial3d(eye_mat.clone()),
                        Transform::from_xyz(0.13, 0.05, 0.28),
                    ));
                    // Pointy ears
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::from_xyz(-0.34, 0.06, 0.0)
                            .with_rotation(Quat::from_rotation_z(PI / 2.5)),
                    ));
                    head.spawn((
                        Mesh3d(ear_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::from_xyz(0.34, 0.06, 0.0)
                            .with_rotation(Quat::from_rotation_z(-PI / 2.5)),
                    ));
                    // Nose
                    head.spawn((
                        Mesh3d(nose_mesh.clone()),
                        MeshMaterial3d(dark_skin.clone()),
                        Transform::from_xyz(0.0, -0.04, 0.30)
                            .with_rotation(Quat::from_rotation_x(PI / 2.0)),
                    ));
                    // Pointy hat
                    head.spawn((
                        Mesh3d(hat_mesh.clone()),
                        MeshMaterial3d(cloth.clone()),
                        Transform::from_xyz(0.0, 0.40, 0.0)
                            .with_rotation(Quat::from_rotation_z(0.15)),
                    ));
                });

            // Left arm (bow arm)
            parent.spawn((
                Mesh3d(arm_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(-0.42, 1.20, 0.14)
                    .with_rotation(Quat::from_rotation_x(-0.4) * Quat::from_rotation_z(0.2)),
            ));
            // Right arm
            parent.spawn((
                Mesh3d(arm_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.42, 1.20, 0.0)
                    .with_rotation(Quat::from_rotation_z(-0.15)),
            ));

            // Legs
            parent.spawn((
                Mesh3d(leg_mesh.clone()),
                MeshMaterial3d(dark_skin.clone()),
                Transform::from_xyz(-0.15, 0.42, 0.0),
            ));
            parent.spawn((
                Mesh3d(leg_mesh.clone()),
                MeshMaterial3d(dark_skin.clone()),
                Transform::from_xyz(0.15, 0.42, 0.0),
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
                    ));
                    // Upper limb (angled outward)
                    bow.spawn((
                        Mesh3d(bow_limb_mesh.clone()),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_xyz(0.0, 0.26, -0.06)
                            .with_rotation(Quat::from_rotation_x(0.25)),
                    ));
                    // Lower limb (angled outward)
                    bow.spawn((
                        Mesh3d(bow_limb_mesh.clone()),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_xyz(0.0, -0.26, -0.06)
                            .with_rotation(Quat::from_rotation_x(-0.25)),
                    ));
                    // String (taut, straight between limb tips)
                    bow.spawn((
                        Mesh3d(bow_string_mesh.clone()),
                        MeshMaterial3d(cloth.clone()),
                        Transform::from_xyz(0.0, 0.0, 0.05),
                    ));
                });
        });
    }
}

fn update_goblin_archers(
    mut commands: Commands,
    time: Res<Time>,
    mut player: Single<
        (&Transform, &PlayerCombat, &Dodge, &mut HitPoints, &mut HitFlash),
        (With<Player>, Without<GoblinArcher>),
    >,
    mut goblins: Query<
        (
            Entity,
            &mut GoblinArcher,
            &mut Transform,
            &mut HitPoints,
            &mut StunMeter,
            &mut HitFlash,
        ),
        Without<Player>,
    >,
    arrow_meshes: Res<ArrowMeshes>,
    mut damage_rng: Local<u64>,
) {
    let delta = time.delta_secs();
    let (player_transform, player_combat, _player_dodge, ref mut _player_health, ref mut _player_flash) =
        *player;
    let player_ground = Vec3::new(
        player_transform.translation.x,
        0.0,
        player_transform.translation.z,
    );
    let player_forward = visual_forward(player_transform).normalize_or_zero();
    let slash_ready = player_combat.strike > 0.42;

    for (entity, mut goblin, mut transform, ref mut goblin_health, ref mut goblin_stun, ref mut goblin_flash) in
        &mut goblins
    {
        goblin.shoot_cooldown = (goblin.shoot_cooldown - delta).max(0.0);
        transform.translation.y = 0.0;

        let goblin_ground =
            Vec3::new(transform.translation.x, 0.0, transform.translation.z);

        // Take damage from player sword
        let to_goblin = goblin_ground - player_ground;
        let goblin_distance = to_goblin.length();
        if slash_ready && player_combat.swing_id != goblin.last_hit_swing_id {
            let goblin_direction = to_goblin.normalize_or_zero();
            if goblin_distance <= PLAYER_SLASH_RANGE
                && goblin_direction.dot(player_forward) >= PLAYER_SLASH_ARC_DOT
            {
                goblin.last_hit_swing_id = player_combat.swing_id;
                let damage = roll_1d5(&mut damage_rng);
                if goblin_health.apply_damage(damage) > 0 {
                    goblin_stun.apply_stun_damage(damage as f32);
                    goblin_flash.trigger();
                    if goblin_health.is_dead() {
                        commands.entity(entity).despawn_recursive();
                        continue;
                    }
                }
            }
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
                // Face movement direction (turns around when fleeing)
                transform.look_to(-move_direction, Vec3::Y);
                goblin.gait_phase += delta * 10.0;
                goblin.move_blend = 1.0;
            } else {
                goblin.move_blend = 0.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Arrows
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct ArrowMeshes {
    shaft: Handle<Mesh>,
    head: Handle<Mesh>,
    shaft_material: Handle<StandardMaterial>,
    head_material: Handle<StandardMaterial>,
}

fn setup_arrow_meshes(
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

fn spawn_arrow(
    commands: &mut Commands,
    meshes: &ArrowMeshes,
    position: Vec3,
    direction: Vec3,
) {
    let rotation = Quat::from_rotation_arc(Vec3::Z, direction.normalize_or_zero());

    commands
        .spawn((
            Arrow {
                direction: direction.normalize_or_zero(),
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
                Transform::from_xyz(0.0, 0.0, 0.35)
                    .with_rotation(Quat::from_rotation_x(PI / 2.0)),
            ));
        });
}

fn update_arrows(
    mut commands: Commands,
    time: Res<Time>,
    mut player: Single<
        (&Transform, &Dodge, &mut HitPoints, &mut HitFlash),
        (With<Player>, Without<Arrow>),
    >,
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

        if distance <= ARROW_HIT_RADIUS && !player_dodge.active {
            if player_health.apply_damage(ARROW_DAMAGE) > 0 {
                player_flash.trigger();
            }
            commands.entity(entity).despawn_recursive();
        }
    }
}

fn handle_respawn_enemies(
    mut commands: Commands,
    mut events: EventReader<RespawnEnemies>,
    rats: Query<Entity, With<DemonRat>>,
    goblins: Query<Entity, With<GoblinArcher>>,
    arrows: Query<Entity, With<Arrow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut triggered = false;
    for _ in events.read() {
        triggered = true;
    }
    if !triggered {
        return;
    }

    // Despawn all enemies and arrows
    for e in &rats {
        commands.entity(e).despawn_recursive();
    }
    for e in &goblins {
        commands.entity(e).despawn_recursive();
    }
    for e in &arrows {
        commands.entity(e).despawn_recursive();
    }

    // Respawn — inline the spawn calls since we can't split ResMut
    do_spawn_rats(&mut commands, &mut meshes, &mut materials);
    do_spawn_goblins(&mut commands, &mut meshes, &mut materials);
}

fn roll_1d5(seed: &mut Local<u64>) -> i32 {
    if **seed == 0 {
        **seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0xA2F1_9C37_5D4B_E821);
    }

    let mut value = **seed;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;

    if value == 0 {
        value = 0x9E37_79B9_7F4A_7C15;
    }

    **seed = value;
    ((value % 5) + 1) as i32
}
