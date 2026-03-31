use std::f32::consts::PI;

use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::combat::{FlashTint, HitFlash, HitPoints, game_running, smoothstep01};
use crate::targeting::{TargetState, Targetable};
use crate::world::tilemap::{
    clamp_ground_target_to_arena, clamp_translation_to_arena, grid_to_world,
};

pub struct PlayerPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum PlayerSet {
    Update,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, PlayerSet::Update)
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (
                    handle_left_click,
                    chase_and_attack_target,
                    trigger_dodge,
                    update_dodge,
                    regen_stamina,
                    move_player,
                    animate_knight,
                )
                    .chain()
                    .run_if(game_running)
                    .in_set(PlayerSet::Update),
            )
            .add_systems(Update, animate_death.after(PlayerSet::Update));
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct MoveTarget {
    pub position: Option<Vec2>,
}

type DodgePlayer<'w> = Single<
    'w,
    (
        &'static Transform,
        &'static mut Dodge,
        &'static mut MoveTarget,
        &'static mut PlayerStats,
        &'static mut KnightAnimator,
        &'static mut PlayerCombat,
    ),
    With<Player>,
>;

#[derive(Component, Default)]
pub struct PlayerCombat {
    pub swing_id: u32,
    pub strike: f32,
}

#[derive(Component)]
pub struct Dodge {
    timer: Timer,
    cooldown: Timer,
    direction: Vec3,
    pub active: bool,
}

impl Default for Dodge {
    fn default() -> Self {
        let mut cooldown = Timer::from_seconds(DODGE_COOLDOWN, TimerMode::Once);
        cooldown.tick(std::time::Duration::from_secs_f32(DODGE_COOLDOWN));
        let mut timer = Timer::from_seconds(DODGE_DURATION, TimerMode::Once);
        timer.tick(std::time::Duration::from_secs_f32(DODGE_DURATION));
        Self {
            timer,
            cooldown,
            direction: Vec3::ZERO,
            active: false,
        }
    }
}

#[derive(Component)]
pub struct PlayerStats {
    pub stamina: f32,
    pub max_stamina: f32,
    pub mana: f32,
    pub max_mana: f32,
    regen_delay: Timer,
}

impl Default for PlayerStats {
    fn default() -> Self {
        let mut delay = Timer::from_seconds(STAMINA_REGEN_DELAY, TimerMode::Once);
        delay.tick(std::time::Duration::from_secs_f32(STAMINA_REGEN_DELAY));
        Self {
            stamina: MAX_STAMINA,
            max_stamina: MAX_STAMINA,
            mana: MAX_MANA,
            max_mana: MAX_MANA,
            regen_delay: delay,
        }
    }
}

impl PlayerStats {
    pub fn spend_stamina(&mut self, amount: f32) -> bool {
        if self.stamina >= amount {
            self.stamina -= amount;
            self.regen_delay.reset();
            true
        } else {
            false
        }
    }
}

const DEATH_ANIM_DURATION: f32 = 1.2;

#[derive(Component)]
pub struct DeathAnim {
    pub timer: Timer,
    pub active: bool,
}

impl Default for DeathAnim {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(DEATH_ANIM_DURATION, TimerMode::Once),
            active: false,
        }
    }
}

#[derive(Component)]
pub(crate) struct KnightAnimator {
    walk_phase: f32,
    swing_timer: Timer,
}

impl Default for KnightAnimator {
    fn default() -> Self {
        Self {
            walk_phase: 0.0,
            swing_timer: Timer::from_seconds(0.75, TimerMode::Once),
        }
    }
}

impl KnightAnimator {
    fn cancel_swing(&mut self) {
        self.swing_timer.set_elapsed(self.swing_timer.duration());
    }
}

#[derive(Default)]
struct AttackIntent {
    pending_strike: bool,
    holding: bool,
    windup: f32,
}

impl AttackIntent {
    fn clear(&mut self) {
        self.pending_strike = false;
        self.holding = false;
        self.windup = 0.0;
    }
}

#[derive(Component, Clone, Copy)]
struct JointRest {
    translation: Vec3,
    rotation: Quat,
}

impl JointRest {
    fn new(translation: Vec3, rotation: Quat) -> Self {
        Self {
            translation,
            rotation,
        }
    }
}

#[derive(Component, Clone, Copy)]
enum KnightJoint {
    Hips,
    Chest,
    Head,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Sword,
}

pub const PLAYER_COLLISION_RADIUS: f32 = 0.78;
const MOVE_SPEED: f32 = 8.0;
const ARRIVE_THRESHOLD: f32 = 0.15;
pub const PLAYER_MAX_HP: i32 = 20;
const ATTACK_RANGE: f32 = 2.3;
const ATTACK_WINDUP: f32 = 0.25;
const DODGE_DURATION: f32 = 0.25;
const DODGE_COOLDOWN: f32 = 0.8;
const DODGE_SPEED: f32 = 18.0;

const MAX_STAMINA: f32 = 60.0;
const MAX_MANA: f32 = 40.0;
const STAMINA_REGEN: f32 = 18.0;
const STAMINA_REGEN_DELAY: f32 = 0.6;
const ATTACK_STAMINA_COST: f32 = 22.0;
const DODGE_STAMINA_COST: f32 = 12.0;

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let start = grid_to_world(10, 10);

    let steel_color = Vec3::new(0.70, 0.74, 0.80);
    let dark_steel_color = Vec3::new(0.18, 0.21, 0.26);
    let trim_color = Vec3::new(0.74, 0.62, 0.28);
    let plume_color = Vec3::new(0.62, 0.16, 0.18);
    let cloth_color = Vec3::new(0.15, 0.20, 0.38);

    let steel = materials.add(StandardMaterial {
        base_color: Color::srgb(steel_color.x, steel_color.y, steel_color.z),
        metallic: 0.12,
        perceptual_roughness: 0.82,
        ..default()
    });
    let dark_steel = materials.add(StandardMaterial {
        base_color: Color::srgb(dark_steel_color.x, dark_steel_color.y, dark_steel_color.z),
        metallic: 0.08,
        perceptual_roughness: 0.92,
        ..default()
    });
    let trim = materials.add(StandardMaterial {
        base_color: Color::srgb(trim_color.x, trim_color.y, trim_color.z),
        metallic: 0.2,
        perceptual_roughness: 0.62,
        ..default()
    });
    let plume = materials.add(StandardMaterial {
        base_color: Color::srgb(plume_color.x, plume_color.y, plume_color.z),
        perceptual_roughness: 0.96,
        ..default()
    });
    let cloth = materials.add(StandardMaterial {
        base_color: Color::srgb(cloth_color.x, cloth_color.y, cloth_color.z),
        perceptual_roughness: 0.98,
        ..default()
    });

    let hips_mesh = meshes.add(Cylinder::new(0.26, 0.24).mesh().resolution(6));
    let torso_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.22,
            radius_bottom: 0.34,
            height: 0.88,
        }
        .mesh()
        .resolution(6),
    );
    let head_mesh = meshes.add(Sphere::new(0.22).mesh().ico(1).unwrap());
    let helmet_mesh = meshes.add(Cone::new(0.30, 0.52).mesh().resolution(6));
    let shoulder_mesh = meshes.add(Sphere::new(0.14).mesh().ico(1).unwrap());
    let arm_mesh = meshes.add(Capsule3d::new(0.09, 0.36).mesh().longitudes(6).latitudes(4));
    let gauntlet_mesh = meshes.add(Cylinder::new(0.11, 0.16).mesh().resolution(6));
    let leg_mesh = meshes.add(Capsule3d::new(0.10, 0.42).mesh().longitudes(6).latitudes(4));
    let boot_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.09,
            radius_bottom: 0.15,
            height: 0.26,
        }
        .mesh()
        .resolution(6),
    );
    let cape_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.18,
            radius_bottom: 0.34,
            height: 0.72,
        }
        .mesh()
        .resolution(5),
    );
    let crest_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.07,
            radius_bottom: 0.12,
            height: 0.58,
        }
        .mesh()
        .resolution(4),
    );
    let visor_mesh = meshes.add(Cylinder::new(0.16, 0.10).mesh().resolution(6).segments(1));
    let plume_mesh = meshes.add(Cone::new(0.06, 0.34).mesh().resolution(5));
    let sword_blade_mesh = meshes.add(
        ConicalFrustum {
            radius_top: 0.045,
            radius_bottom: 0.015,
            height: 0.76,
        }
        .mesh()
        .resolution(4),
    );
    let sword_handle_mesh = meshes.add(Cylinder::new(0.035, 0.26).mesh().resolution(4));
    let sword_guard_mesh = meshes.add(Cylinder::new(0.025, 0.34).mesh().resolution(4));

    let player = commands
        .spawn((
            Player,
            MoveTarget { position: None },
            PlayerCombat::default(),
            Dodge::default(),
            PlayerStats::default(),
            DeathAnim::default(),
            HitPoints::new(PLAYER_MAX_HP),
            HitFlash::default(),
            KnightAnimator::default(),
            Transform::from_translation(start),
            Visibility::Visible,
        ))
        .id();

    commands.entity(player).with_children(|parent| {
        parent
            .spawn((
                KnightJoint::Hips,
                JointRest::new(Vec3::new(0.0, 0.98, 0.0), Quat::IDENTITY),
                Transform::from_xyz(0.0, 0.98, 0.0),
                Visibility::Inherited,
            ))
            .with_children(|hips| {
                hips.spawn((
                    Mesh3d(hips_mesh.clone()),
                    MeshMaterial3d(dark_steel.clone()),
                    Transform::IDENTITY,
                    FlashTint {
                        owner: player,
                        base_srgb: dark_steel_color,
                    },
                ));

                hips.spawn((
                    Mesh3d(cape_mesh.clone()),
                    MeshMaterial3d(cloth.clone()),
                    Transform::from_xyz(0.0, 0.18, -0.26)
                        .with_rotation(Quat::from_rotation_x(PI))
                        .with_scale(Vec3::new(0.95, 1.0, 0.75)),
                    FlashTint {
                        owner: player,
                        base_srgb: cloth_color,
                    },
                ));

                hips.spawn((
                    KnightJoint::LeftLeg,
                    JointRest::new(Vec3::new(-0.15, -0.06, 0.0), Quat::IDENTITY),
                    Transform::from_xyz(-0.15, -0.06, 0.0),
                    Visibility::Inherited,
                ))
                .with_children(|leg| {
                    leg.spawn((
                        Mesh3d(leg_mesh.clone()),
                        MeshMaterial3d(dark_steel.clone()),
                        Transform::from_xyz(0.0, -0.34, 0.0),
                    ));
                    leg.spawn((
                        Mesh3d(boot_mesh.clone()),
                        MeshMaterial3d(trim.clone()),
                        Transform::from_xyz(0.0, -0.74, 0.05)
                            .with_rotation(Quat::from_rotation_x(-PI / 2.0))
                            .with_scale(Vec3::new(1.0, 1.0, 1.25)),
                    ));
                });

                hips.spawn((
                    KnightJoint::RightLeg,
                    JointRest::new(Vec3::new(0.15, -0.06, 0.0), Quat::IDENTITY),
                    Transform::from_xyz(0.15, -0.06, 0.0),
                    Visibility::Inherited,
                ))
                .with_children(|leg| {
                    leg.spawn((
                        Mesh3d(leg_mesh.clone()),
                        MeshMaterial3d(dark_steel.clone()),
                        Transform::from_xyz(0.0, -0.34, 0.0),
                    ));
                    leg.spawn((
                        Mesh3d(boot_mesh.clone()),
                        MeshMaterial3d(trim.clone()),
                        Transform::from_xyz(0.0, -0.74, 0.05)
                            .with_rotation(Quat::from_rotation_x(-PI / 2.0))
                            .with_scale(Vec3::new(1.0, 1.0, 1.25)),
                    ));
                });

                hips.spawn((
                    KnightJoint::Chest,
                    JointRest::new(Vec3::new(0.0, 0.38, 0.0), Quat::IDENTITY),
                    Transform::from_xyz(0.0, 0.38, 0.0),
                    Visibility::Inherited,
                ))
                .with_children(|chest| {
                    chest.spawn((
                        Mesh3d(torso_mesh.clone()),
                        MeshMaterial3d(steel.clone()),
                        Transform::from_xyz(0.0, 0.30, 0.0),
                        FlashTint {
                            owner: player,
                            base_srgb: steel_color,
                        },
                    ));
                    chest.spawn((
                        Mesh3d(crest_mesh.clone()),
                        MeshMaterial3d(trim.clone()),
                        Transform::from_xyz(0.0, 0.20, 0.27)
                            .with_rotation(Quat::from_rotation_x(PI)),
                        FlashTint {
                            owner: player,
                            base_srgb: trim_color,
                        },
                    ));

                    chest
                        .spawn((
                            KnightJoint::Head,
                            JointRest::new(Vec3::new(0.0, 0.78, 0.0), Quat::IDENTITY),
                            Transform::from_xyz(0.0, 0.78, 0.0),
                            Visibility::Inherited,
                        ))
                        .with_children(|head| {
                            head.spawn((
                                Mesh3d(head_mesh.clone()),
                                MeshMaterial3d(steel.clone()),
                                Transform::from_xyz(0.0, 0.04, 0.0),
                            ));
                            head.spawn((
                                Mesh3d(helmet_mesh.clone()),
                                MeshMaterial3d(trim.clone()),
                                Transform::from_xyz(0.0, 0.26, 0.0),
                            ));
                            head.spawn((
                                Mesh3d(visor_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(0.0, 0.07, 0.18)
                                    .with_rotation(Quat::from_rotation_x(PI / 2.0))
                                    .with_scale(Vec3::new(1.0, 0.55, 1.0)),
                            ));
                            head.spawn((
                                Mesh3d(plume_mesh.clone()),
                                MeshMaterial3d(plume.clone()),
                                Transform::from_xyz(0.0, 0.56, -0.03)
                                    .with_rotation(Quat::from_rotation_x(-0.42)),
                                FlashTint {
                                    owner: player,
                                    base_srgb: plume_color,
                                },
                            ));
                        });

                    chest
                        .spawn((
                            KnightJoint::LeftArm,
                            JointRest::new(
                                Vec3::new(-0.42, 0.50, 0.0),
                                Quat::from_rotation_z(0.14),
                            ),
                            Transform::from_translation(Vec3::new(-0.42, 0.50, 0.0))
                                .with_rotation(Quat::from_rotation_z(0.14)),
                            Visibility::Inherited,
                        ))
                        .with_children(|arm| {
                            arm.spawn((
                                Mesh3d(shoulder_mesh.clone()),
                                MeshMaterial3d(trim.clone()),
                                Transform::IDENTITY,
                            ));
                            arm.spawn((
                                Mesh3d(arm_mesh.clone()),
                                MeshMaterial3d(steel.clone()),
                                Transform::from_xyz(0.0, -0.28, 0.0),
                            ));
                            arm.spawn((
                                Mesh3d(gauntlet_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(0.0, -0.58, 0.0),
                            ));
                        });

                    chest
                        .spawn((
                            KnightJoint::RightArm,
                            JointRest::new(
                                Vec3::new(0.42, 0.50, 0.0),
                                Quat::from_rotation_x(-0.18)
                                    * Quat::from_rotation_y(-0.10)
                                    * Quat::from_rotation_z(0.48),
                            ),
                            Transform::from_translation(Vec3::new(0.42, 0.50, 0.0)).with_rotation(
                                Quat::from_rotation_x(-0.18)
                                    * Quat::from_rotation_y(-0.10)
                                    * Quat::from_rotation_z(0.48),
                            ),
                            Visibility::Inherited,
                        ))
                        .with_children(|arm| {
                            arm.spawn((
                                Mesh3d(shoulder_mesh.clone()),
                                MeshMaterial3d(trim.clone()),
                                Transform::IDENTITY,
                            ));
                            arm.spawn((
                                Mesh3d(arm_mesh.clone()),
                                MeshMaterial3d(steel.clone()),
                                Transform::from_xyz(0.0, -0.28, 0.0),
                            ));
                            arm.spawn((
                                Mesh3d(gauntlet_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(0.0, -0.58, 0.0),
                            ));
                            arm.spawn((
                                KnightJoint::Sword,
                                JointRest::new(
                                    Vec3::new(0.08, -0.66, 0.14),
                                    Quat::from_rotation_x(-0.78)
                                        * Quat::from_rotation_y(-0.10)
                                        * Quat::from_rotation_z(-0.08),
                                ),
                                Transform::from_xyz(0.08, -0.66, 0.14).with_rotation(
                                    Quat::from_rotation_x(-0.78)
                                        * Quat::from_rotation_y(-0.10)
                                        * Quat::from_rotation_z(-0.08),
                                ),
                                Visibility::Inherited,
                            ))
                            .with_children(|sword| {
                                sword.spawn((
                                    Mesh3d(sword_handle_mesh.clone()),
                                    MeshMaterial3d(dark_steel.clone()),
                                    Transform::from_xyz(0.0, -0.10, 0.0),
                                ));
                                sword.spawn((
                                    Mesh3d(sword_guard_mesh.clone()),
                                    MeshMaterial3d(trim.clone()),
                                    Transform::from_xyz(0.0, -0.22, 0.0)
                                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                                ));
                                sword.spawn((
                                    Mesh3d(sword_blade_mesh.clone()),
                                    MeshMaterial3d(steel.clone()),
                                    Transform::from_xyz(0.0, -0.64, 0.0),
                                ));
                            });
                        });
                });
            });
    });
}

fn handle_left_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera_query: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut player: Single<&mut MoveTarget, With<Player>>,
    mut target_state: ResMut<TargetState>,
) {
    // Click on enemy: set target
    if mouse_buttons.just_pressed(MouseButton::Left) {
        if let Some(hovered) = target_state.hovered {
            target_state.targeted = Some(hovered);
            player.position = None;
            return;
        }

        target_state.targeted = None;
    }

    // Hold on ground: continuously move toward cursor
    if !mouse_buttons.pressed(MouseButton::Left) {
        return;
    }

    // Don't override movement while chasing a target
    if target_state.targeted.is_some() {
        return;
    }

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let (camera, cam_transform) = *camera_query;

    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
        return;
    };
    let Some(distance) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };

    let point = ray.get_point(distance);
    player.position = Some(clamp_ground_target_to_arena(
        Vec2::new(point.x, point.z),
        PLAYER_COLLISION_RADIUS,
    ));
}

fn chase_and_attack_target(
    target_state: Res<TargetState>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    target_transforms: Query<&GlobalTransform, With<Targetable>>,
    mut player: Single<
        (
            &mut Transform,
            &mut MoveTarget,
            &mut KnightAnimator,
            &mut PlayerCombat,
            &mut PlayerStats,
        ),
        With<Player>,
    >,
    mut attack_intent: Local<AttackIntent>,
    time: Res<Time>,
) {
    let Some(target_entity) = target_state.targeted else {
        attack_intent.clear();
        return;
    };
    let Ok(target_tf) = target_transforms.get(target_entity) else {
        attack_intent.clear();
        return;
    };

    // Click sets a pending strike that survives the walk; hold = continuous
    if mouse_buttons.just_pressed(MouseButton::Left) {
        attack_intent.pending_strike = true;
        attack_intent.holding = true;
    }
    if !mouse_buttons.pressed(MouseButton::Left) {
        attack_intent.holding = false;
    }

    let (ref mut player_tf, ref mut move_target, ref mut animator, ref mut combat, ref mut stats) =
        *player;
    let target_pos = target_tf.translation();
    let to_target = Vec3::new(
        target_pos.x - player_tf.translation.x,
        0.0,
        target_pos.z - player_tf.translation.z,
    );
    let distance = to_target.length();

    // Face the target
    if distance > 0.001 {
        player_tf.look_to(-to_target.normalize(), Vec3::Y);
    }

    let can_attack = attack_intent.pending_strike || attack_intent.holding;

    if distance > ATTACK_RANGE {
        // Chase: move toward the target, keep winding up if close-ish
        move_target.position = Some(clamp_ground_target_to_arena(
            Vec2::new(target_pos.x, target_pos.z),
            PLAYER_COLLISION_RADIUS,
        ));
        if !can_attack || distance > ATTACK_RANGE * 1.5 {
            attack_intent.windup = 0.0;
        }
    } else {
        // In range: stop and plant feet
        move_target.position = None;
    }

    if can_attack && animator.swing_timer.finished() {
        attack_intent.windup += time.delta_secs();
        if attack_intent.windup >= ATTACK_WINDUP
            && distance <= ATTACK_RANGE
            && stats.spend_stamina(ATTACK_STAMINA_COST)
        {
            attack_intent.pending_strike = false;
            attack_intent.windup = 0.0;
            animator.swing_timer.reset();
            combat.swing_id = combat.swing_id.wrapping_add(1);
            combat.strike = 0.0;
        }
    } else if !can_attack {
        attack_intent.windup = 0.0;
    }
}

fn trigger_dodge(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player: DodgePlayer<'_>,
    mut target_state: ResMut<TargetState>,
) {
    let (
        player_tf,
        ref mut dodge,
        ref mut move_target,
        ref mut stats,
        ref mut animator,
        ref mut combat,
    ) = *player;

    if !keyboard.just_pressed(KeyCode::Space) || !dodge.cooldown.finished() || dodge.active {
        return;
    }

    if !stats.spend_stamina(DODGE_STAMINA_COST) {
        return;
    }

    // Dodge in current facing direction
    let forward = visual_forward(player_tf).normalize_or_zero();
    let direction = if forward.length_squared() > 0.001 {
        Vec3::new(forward.x, 0.0, forward.z).normalize()
    } else {
        Vec3::Z
    };

    dodge.active = true;
    dodge.direction = direction;
    dodge.timer.reset();
    dodge.cooldown.reset();

    // Cancel current actions
    animator.cancel_swing();
    combat.strike = 0.0;
    move_target.position = None;
    target_state.targeted = None;
}

fn update_dodge(time: Res<Time>, mut player: Single<(&mut Transform, &mut Dodge), With<Player>>) {
    let (ref mut player_tf, ref mut dodge) = *player;

    dodge.timer.tick(time.delta());
    dodge.cooldown.tick(time.delta());

    if !dodge.active {
        return;
    }

    if dodge.timer.finished() {
        dodge.active = false;
        return;
    }

    // Move in dodge direction
    let step = DODGE_SPEED * time.delta_secs();
    player_tf.translation += dodge.direction * step;
    clamp_translation_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
}

fn regen_stamina(time: Res<Time>, mut player: Single<&mut PlayerStats, With<Player>>) {
    player.regen_delay.tick(time.delta());
    if player.regen_delay.finished() && player.stamina < player.max_stamina {
        player.stamina =
            (player.stamina + STAMINA_REGEN * time.delta_secs()).min(player.max_stamina);
    }
}

fn move_player(
    mut player_query: Single<(&mut Transform, &mut MoveTarget, &Dodge), With<Player>>,
    time: Res<Time>,
) {
    let (ref mut player_tf, ref mut move_target, dodge) = *player_query;

    // Don't move normally during dodge
    if dodge.active {
        return;
    }

    let Some(target_position) = move_target.position else {
        return;
    };

    let current = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let diff = target_position - current;
    let distance = diff.length();

    if distance <= ARRIVE_THRESHOLD {
        move_target.position = None;
        return;
    }

    let direction = diff / distance;
    let facing = Vec3::new(direction.x, 0.0, direction.y);
    if facing.length_squared() > 0.0 {
        // The knight mesh is built facing +Z, but look_to aligns local -Z to the target.
        player_tf.look_to(-facing, Vec3::Y);
    }

    let step_mag = MOVE_SPEED * time.delta_secs();
    if step_mag >= distance {
        player_tf.translation.x = target_position.x;
        player_tf.translation.z = target_position.y;
        move_target.position = None;
    } else {
        player_tf.translation.x += direction.x * step_mag;
        player_tf.translation.z += direction.y * step_mag;
    }

    clamp_translation_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
}

fn animate_knight(
    time: Res<Time>,
    mut player: Single<(&MoveTarget, &Dodge, &mut KnightAnimator, &mut PlayerCombat), With<Player>>,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let (move_target, dodge, ref mut animator, ref mut combat) = *player;

    if dodge.active {
        animator.cancel_swing();
        combat.strike = 0.0;
    } else {
        animator.swing_timer.tick(time.delta());
    }

    if move_target.position.is_some() {
        animator.walk_phase += time.delta_secs() * 8.0;
    }

    let moving = move_target.position.is_some();
    let walk_swing = if moving {
        animator.walk_phase.sin()
    } else {
        0.0
    };
    let walk_bob = if moving {
        (animator.walk_phase * 2.0).sin() * 0.06
    } else {
        0.0
    };
    let sword_swing = if dodge.active {
        0.0
    } else {
        sword_swing_curve(&animator.swing_timer)
    };
    let sword_windup = (-sword_swing).max(0.0);
    let sword_strike = sword_swing.max(0.0);
    combat.strike = sword_strike;

    // Dodge roll: 0.0 → 1.0 over DODGE_DURATION
    let dodge_t = if dodge.active {
        let elapsed = dodge.timer.elapsed_secs();
        let duration = dodge.timer.duration().as_secs_f32().max(0.001);
        (elapsed / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Full forward tumble: smoothstep for natural acceleration/deceleration
    let roll_angle = if dodge.active {
        smoothstep01(dodge_t) * PI * 2.0
    } else {
        0.0
    };
    // Drop hips low during the middle of the roll
    let roll_crouch = if dodge.active {
        (dodge_t * PI).sin() * 0.55
    } else {
        0.0
    };
    // Tuck limbs during roll
    let roll_tuck = if dodge.active {
        (dodge_t * PI).sin()
    } else {
        0.0
    };

    for (joint, rest, mut transform) in &mut joints {
        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        if dodge.active {
            match joint {
                KnightJoint::Hips => {
                    transform.translation.y -= roll_crouch;
                    transform.rotation *= Quat::from_rotation_x(roll_angle);
                }
                KnightJoint::Chest => {}
                KnightJoint::Head => {
                    transform.rotation *= Quat::from_rotation_x(-roll_tuck * 0.6);
                }
                KnightJoint::LeftArm => {
                    transform.rotation *= Quat::from_rotation_x(roll_tuck * 1.2)
                        * Quat::from_rotation_z(-roll_tuck * 0.3);
                }
                KnightJoint::RightArm => {
                    transform.rotation *= Quat::from_rotation_x(roll_tuck * 1.2)
                        * Quat::from_rotation_z(roll_tuck * 0.3);
                }
                KnightJoint::LeftLeg => {
                    transform.rotation *= Quat::from_rotation_x(-roll_tuck * 0.9);
                }
                KnightJoint::RightLeg => {
                    transform.rotation *= Quat::from_rotation_x(-roll_tuck * 0.9);
                }
                KnightJoint::Sword => {}
            }
            continue;
        }

        match joint {
            KnightJoint::Hips => {
                transform.translation.y += walk_bob;
                transform.rotation *= Quat::from_rotation_x(-sword_strike * 0.04)
                    * Quat::from_rotation_y(
                        walk_swing * 0.05 - sword_windup * 0.06 + sword_strike * 0.10,
                    );
            }
            KnightJoint::Chest => {
                transform.translation.y += walk_bob * 0.4;
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 0.16 - sword_strike * 0.28)
                        * Quat::from_rotation_y(
                            -walk_swing * 0.08 - sword_windup * 0.10 + sword_strike * 0.12,
                        )
                        * Quat::from_rotation_z(-sword_windup * 0.04 + sword_strike * 0.06);
            }
            KnightJoint::Head => {
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 0.04 - sword_strike * 0.05)
                        * Quat::from_rotation_y(sword_windup * 0.03 - sword_strike * 0.05);
            }
            KnightJoint::LeftArm => {
                transform.rotation *=
                    Quat::from_rotation_x(
                        -walk_swing * 0.45 + sword_windup * 0.08 - sword_strike * 0.04,
                    ) * Quat::from_rotation_z(sword_windup * 0.04 + sword_strike * 0.06);
            }
            KnightJoint::RightArm => {
                transform.rotation *=
                    Quat::from_rotation_x(
                        walk_swing * 0.12 + sword_windup * 1.86 - sword_strike * 2.48,
                    ) * Quat::from_rotation_y(sword_windup * 0.22 - sword_strike * 0.16)
                        * Quat::from_rotation_z(sword_windup * 0.10 - sword_strike * 0.18);
            }
            KnightJoint::LeftLeg => {
                transform.rotation *= Quat::from_rotation_x(walk_swing * 0.55);
            }
            KnightJoint::RightLeg => {
                transform.rotation *= Quat::from_rotation_x(-walk_swing * 0.55);
            }
            KnightJoint::Sword => {
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 1.22 - sword_strike * 1.72)
                        * Quat::from_rotation_y(-sword_windup * 0.06 + sword_strike * 0.08)
                        * Quat::from_rotation_z(-sword_windup * 0.04 - sword_strike * 0.72);
            }
        }
    }
}

fn sword_swing_curve(timer: &Timer) -> f32 {
    if timer.finished() {
        return 0.0;
    }

    let duration = timer.duration().as_secs_f32();
    if duration <= 0.0 {
        return 0.0;
    }

    let t = timer.elapsed_secs() / duration;
    if t < 0.24 {
        -0.68 * smoothstep01(t / 0.24)
    } else if t < 0.54 {
        let strike_t = (t - 0.24) / 0.30;
        -0.68 + 1.96 * smoothstep01(strike_t)
    } else {
        let recover_t = (t - 0.54) / 0.46;
        1.28 * (1.0 - smoothstep01(recover_t))
    }
}

fn animate_death(
    time: Res<Time>,
    mut player: Query<(&mut DeathAnim, &HitPoints), With<Player>>,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let Ok((mut death_anim, hp)) = player.get_single_mut() else {
        return;
    };

    if !hp.is_dead() {
        return;
    }

    if !death_anim.active {
        death_anim.active = true;
        death_anim.timer.reset();
    }

    death_anim.timer.tick(time.delta());

    let t = (death_anim.timer.elapsed_secs() / DEATH_ANIM_DURATION).clamp(0.0, 1.0);
    let fall = smoothstep01(t);

    // Fall sideways to the ground
    let fall_rotation = fall * (PI / 2.0); // 90 degrees to the side
    let fall_drop = fall * 0.7; // hips drop toward ground

    for (joint, rest, mut transform) in &mut joints {
        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        match joint {
            KnightJoint::Hips => {
                transform.translation.y -= fall_drop;
                transform.rotation *= Quat::from_rotation_z(fall_rotation);
            }
            KnightJoint::Head => {
                // Head lolls forward
                transform.rotation *= Quat::from_rotation_x(fall * 0.4);
            }
            KnightJoint::LeftArm => {
                transform.rotation *=
                    Quat::from_rotation_x(-fall * 0.6) * Quat::from_rotation_z(-fall * 0.3);
            }
            KnightJoint::RightArm => {
                transform.rotation *=
                    Quat::from_rotation_x(-fall * 0.8) * Quat::from_rotation_z(fall * 0.4);
            }
            KnightJoint::Sword => {
                // Sword drops away
                transform.rotation *=
                    Quat::from_rotation_x(-fall * 1.2) * Quat::from_rotation_z(fall * 0.6);
            }
            _ => {}
        }
    }
}

pub fn visual_forward(transform: &Transform) -> Vec3 {
    transform.rotation * Vec3::Z
}
