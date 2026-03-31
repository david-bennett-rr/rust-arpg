use std::f32::consts::PI;

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use crate::camera::MainCamera;
use crate::combat::{FlashTint, HitFlash, HitPoints};
use crate::world::tilemap::grid_to_world;

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
                    set_move_target_on_click.run_if(input_just_pressed(MouseButton::Left)),
                    trigger_sword_swing,
                    move_player,
                    animate_knight,
                )
                    .chain()
                    .in_set(PlayerSet::Update),
            );
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct MoveTarget {
    pub position: Option<Vec2>,
}

#[derive(Component, Default)]
pub struct PlayerCombat {
    pub swing_id: u32,
    pub strike: f32,
}

#[derive(Component)]
struct KnightAnimator {
    walk_phase: f32,
    swing_timer: Timer,
}

impl Default for KnightAnimator {
    fn default() -> Self {
        Self {
            walk_phase: 0.0,
            swing_timer: Timer::from_seconds(0.38, TimerMode::Once),
        }
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

const MOVE_SPEED: f32 = 8.0;
const ARRIVE_THRESHOLD: f32 = 0.15;
const PLAYER_MAX_HP: i32 = 20;

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

fn set_move_target_on_click(
    window: Single<&Window>,
    camera_query: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut move_target: Single<&mut MoveTarget, With<Player>>,
) {
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
    move_target.position = Some(Vec2::new(point.x, point.z));
}

fn trigger_sword_swing(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player: Single<(&mut KnightAnimator, &mut PlayerCombat), With<Player>>,
) {
    let (ref mut animator, ref mut combat) = *player;

    if (mouse_buttons.just_pressed(MouseButton::Right) || keyboard.just_pressed(KeyCode::Space))
        && animator.swing_timer.finished()
    {
        animator.swing_timer.reset();
        combat.swing_id = combat.swing_id.wrapping_add(1);
        combat.strike = 0.0;
    }
}

fn move_player(
    mut player_query: Single<(&mut Transform, &mut MoveTarget), With<Player>>,
    time: Res<Time>,
) {
    let (ref mut player_tf, ref mut move_target) = *player_query;
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
}

fn animate_knight(
    time: Res<Time>,
    mut player: Single<(&MoveTarget, &mut KnightAnimator, &mut PlayerCombat), With<Player>>,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let (move_target, ref mut animator, ref mut combat) = *player;

    animator.swing_timer.tick(time.delta());

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
    let sword_swing = sword_swing_curve(&animator.swing_timer);
    let sword_windup = (-sword_swing).max(0.0);
    let sword_strike = sword_swing.max(0.0);
    combat.strike = sword_strike;

    for (joint, rest, mut transform) in &mut joints {
        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

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

fn smoothstep01(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub fn visual_forward(transform: &Transform) -> Vec3 {
    transform.rotation * Vec3::Z
}
