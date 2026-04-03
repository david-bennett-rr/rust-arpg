use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{FlashTint, HitFlash, HitPoints};
use crate::world::floor::FloorMap;
use crate::world::tilemap::{clamp_ground_target, FloorBounds, WallSpatialIndex};

use super::{
    AttackLunge, ControllerMove, DeathAnim, Dodge, HealingFlask, JointRest, KnightAnimator,
    KnightJoint, MoveTarget, Player, PlayerCombat, PlayerStats, RunState, PLAYER_COLLISION_RADIUS,
    PLAYER_MAX_HP,
};

pub(super) fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    floor_map: Res<FloorMap>,
    bounds: Res<FloorBounds>,
    wall_index: Res<WallSpatialIndex>,
) {
    let bonfire_pos = floor_map.rooms[floor_map.start_room].world_center;
    let start = safe_bonfire_spawn_position(bonfire_pos, &bounds, &wall_index);

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
    let shield_face_mesh = meshes.add(Cylinder::new(0.26, 0.04).mesh().resolution(8));
    let shield_rim_mesh = meshes.add(Cylinder::new(0.29, 0.02).mesh().resolution(8));
    let shield_boss_mesh = meshes.add(Sphere::new(0.07).mesh().ico(1).unwrap());

    let player = commands
        .spawn((
            Player,
            MoveTarget { position: None },
            RunState::default(),
            ControllerMove::default(),
            PlayerCombat::default(),
            AttackLunge::default(),
            Dodge::default(),
            PlayerStats::default(),
            HealingFlask::default(),
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
                        FlashTint {
                            owner: player,
                            base_srgb: dark_steel_color,
                        },
                    ));
                    leg.spawn((
                        Mesh3d(boot_mesh.clone()),
                        MeshMaterial3d(trim.clone()),
                        Transform::from_xyz(0.0, -0.74, 0.05)
                            .with_rotation(Quat::from_rotation_x(-PI / 2.0))
                            .with_scale(Vec3::new(1.0, 1.0, 1.25)),
                        FlashTint {
                            owner: player,
                            base_srgb: trim_color,
                        },
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
                        FlashTint {
                            owner: player,
                            base_srgb: dark_steel_color,
                        },
                    ));
                    leg.spawn((
                        Mesh3d(boot_mesh.clone()),
                        MeshMaterial3d(trim.clone()),
                        Transform::from_xyz(0.0, -0.74, 0.05)
                            .with_rotation(Quat::from_rotation_x(-PI / 2.0))
                            .with_scale(Vec3::new(1.0, 1.0, 1.25)),
                        FlashTint {
                            owner: player,
                            base_srgb: trim_color,
                        },
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
                                FlashTint {
                                    owner: player,
                                    base_srgb: steel_color,
                                },
                            ));
                            head.spawn((
                                Mesh3d(helmet_mesh.clone()),
                                MeshMaterial3d(trim.clone()),
                                Transform::from_xyz(0.0, 0.26, 0.0),
                                FlashTint {
                                    owner: player,
                                    base_srgb: trim_color,
                                },
                            ));
                            head.spawn((
                                Mesh3d(visor_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(0.0, 0.07, 0.18)
                                    .with_rotation(Quat::from_rotation_x(PI / 2.0))
                                    .with_scale(Vec3::new(1.0, 0.55, 1.0)),
                                FlashTint {
                                    owner: player,
                                    base_srgb: dark_steel_color,
                                },
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
                                FlashTint {
                                    owner: player,
                                    base_srgb: trim_color,
                                },
                            ));
                            arm.spawn((
                                Mesh3d(arm_mesh.clone()),
                                MeshMaterial3d(steel.clone()),
                                Transform::from_xyz(0.0, -0.28, 0.0),
                                FlashTint {
                                    owner: player,
                                    base_srgb: steel_color,
                                },
                            ));
                            arm.spawn((
                                Mesh3d(gauntlet_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(0.0, -0.58, 0.0),
                                FlashTint {
                                    owner: player,
                                    base_srgb: dark_steel_color,
                                },
                            ));
                            // Shield
                            arm.spawn((
                                Mesh3d(shield_rim_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(-0.06, -0.30, 0.16)
                                    .with_rotation(Quat::from_rotation_x(PI / 2.0))
                                    .with_scale(Vec3::new(1.0, 1.0, 0.85)),
                                FlashTint {
                                    owner: player,
                                    base_srgb: dark_steel_color,
                                },
                            ));
                            arm.spawn((
                                Mesh3d(shield_face_mesh.clone()),
                                MeshMaterial3d(steel.clone()),
                                Transform::from_xyz(-0.06, -0.30, 0.16)
                                    .with_rotation(Quat::from_rotation_x(PI / 2.0))
                                    .with_scale(Vec3::new(1.0, 1.0, 0.85)),
                                FlashTint {
                                    owner: player,
                                    base_srgb: steel_color,
                                },
                            ));
                            arm.spawn((
                                Mesh3d(shield_boss_mesh.clone()),
                                MeshMaterial3d(trim.clone()),
                                Transform::from_xyz(-0.06, -0.30, 0.20)
                                    .with_scale(Vec3::new(1.0, 0.65, 1.0)),
                                FlashTint {
                                    owner: player,
                                    base_srgb: trim_color,
                                },
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
                                FlashTint {
                                    owner: player,
                                    base_srgb: trim_color,
                                },
                            ));
                            arm.spawn((
                                Mesh3d(arm_mesh.clone()),
                                MeshMaterial3d(steel.clone()),
                                Transform::from_xyz(0.0, -0.28, 0.0),
                                FlashTint {
                                    owner: player,
                                    base_srgb: steel_color,
                                },
                            ));
                            arm.spawn((
                                Mesh3d(gauntlet_mesh.clone()),
                                MeshMaterial3d(dark_steel.clone()),
                                Transform::from_xyz(0.0, -0.58, 0.0),
                                FlashTint {
                                    owner: player,
                                    base_srgb: dark_steel_color,
                                },
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
                                    FlashTint {
                                        owner: player,
                                        base_srgb: dark_steel_color,
                                    },
                                ));
                                sword.spawn((
                                    Mesh3d(sword_guard_mesh.clone()),
                                    MeshMaterial3d(trim.clone()),
                                    Transform::from_xyz(0.0, -0.22, 0.0)
                                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                                    FlashTint {
                                        owner: player,
                                        base_srgb: trim_color,
                                    },
                                ));
                                sword.spawn((
                                    Mesh3d(sword_blade_mesh.clone()),
                                    MeshMaterial3d(steel.clone()),
                                    Transform::from_xyz(0.0, -0.64, 0.0),
                                    FlashTint {
                                        owner: player,
                                        base_srgb: steel_color,
                                    },
                                ));
                            });
                        });
                });
            });
    });
}

fn safe_bonfire_spawn_position(
    bonfire_pos: Vec3,
    bounds: &FloorBounds,
    wall_index: &WallSpatialIndex,
) -> Vec3 {
    let bonfire_ground = Vec2::new(bonfire_pos.x, bonfire_pos.z);
    let distances = [3.05, 3.45, 2.75, 4.10];
    let angle_offsets = [0.0, 0.45, -0.45, 0.9, -0.9, 1.35, -1.35, PI];

    for distance in distances {
        for angle_offset in angle_offsets {
            let direction = Vec2::from_angle(-PI / 2.0 + angle_offset);
            let candidate = clamp_ground_target(
                bounds,
                bonfire_ground + direction * distance,
                PLAYER_COLLISION_RADIUS,
            );
            if bonfire_spawn_position_clear(candidate, wall_index) {
                return Vec3::new(candidate.x, bonfire_pos.y, candidate.y);
            }
        }
    }

    let fallback = clamp_ground_target(
        bounds,
        bonfire_ground + Vec2::NEG_Y * distances[0],
        PLAYER_COLLISION_RADIUS,
    );
    Vec3::new(fallback.x, bonfire_pos.y, fallback.y)
}

fn bonfire_spawn_position_clear(point: Vec2, wall_index: &WallSpatialIndex) -> bool {
    let radius = PLAYER_COLLISION_RADIUS + 0.08;
    let mut blocked = false;

    wall_index.for_each_nearby_segment(point, radius, |segment| {
        if blocked {
            return;
        }

        let closest_x = point.x.clamp(
            segment.center.x - segment.half_extents.x,
            segment.center.x + segment.half_extents.x,
        );
        let closest_z = point.y.clamp(
            segment.center.y - segment.half_extents.y,
            segment.center.y + segment.half_extents.y,
        );
        let delta = point - Vec2::new(closest_x, closest_z);
        if delta.length_squared() < radius * radius {
            blocked = true;
        }
    });

    !blocked
}
