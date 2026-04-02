pub mod floor;
pub mod fog;
pub mod room;
pub mod tilemap;

use bevy::{pbr::MaterialPlugin, prelude::*};

use crate::enemy;
use crate::world::room::RoomTag;
use floor::generate_floor;
use tilemap::{
    spawn_corridor_floor, spawn_room_floor, spawn_room_walls, FloorBounds, FloorSceneAssets,
    WallSegment, WallSpatialIndex,
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<fog::FogOverlayMaterial>::default())
            .init_resource::<FloorBounds>()
            .init_resource::<fog::FogRuntimeState>()
            .init_resource::<WallSpatialIndex>()
            .add_systems(Startup, spawn_floor)
            .add_systems(PostUpdate, tilemap::resolve_wall_collisions)
            .add_systems(
                PostUpdate,
                (fog::update_static_fog, fog::update_dynamic_fog)
                    .chain()
                    .after(tilemap::resolve_wall_collisions),
            );
    }
}

/// Spawn the entire first floor: all rooms, corridors, walls, enemies, lighting.
fn spawn_floor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fog_materials: ResMut<Assets<fog::FogOverlayMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bounds: ResMut<FloorBounds>,
    mut wall_index: ResMut<WallSpatialIndex>,
) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let floor_map = generate_floor(seed);
    let scene_assets = FloorSceneAssets::new(&mut meshes, &mut materials);
    let mut wall_segments: Vec<WallSegment> = Vec::new();

    // Spawn room floors and walls
    for placed in &floor_map.rooms {
        let template = &floor_map.templates[placed.template_index];
        let connected_sides: Vec<room::DoorSide> =
            placed.connections.iter().map(|(_, side)| *side).collect();
        spawn_room_floor(&mut commands, &scene_assets, template, placed.world_center);
        spawn_room_walls(
            &mut commands,
            &scene_assets,
            template,
            placed.world_center,
            &connected_sides,
            &mut wall_segments,
        );
    }

    // Spawn corridors
    for corridor in &floor_map.corridors {
        spawn_corridor_floor(
            &mut commands,
            &scene_assets,
            corridor.start,
            corridor.end,
            corridor.width,
            &mut wall_segments,
        );
    }
    wall_index.rebuild(wall_segments);

    // Bounds = AABB of the entire floor (rooms + corridors)
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    for placed in &floor_map.rooms {
        let template = &floor_map.templates[placed.template_index];
        let cx = placed.world_center.x;
        let cz = placed.world_center.z;
        min_x = min_x.min(cx - template.half_extent_x());
        max_x = max_x.max(cx + template.half_extent_x());
        min_z = min_z.min(cz - template.half_extent_z());
        max_z = max_z.max(cz + template.half_extent_z());
    }
    for corridor in &floor_map.corridors {
        let half_w = corridor.width as f32 * tilemap::TILE_SIZE * 0.5;
        for endpoint in [corridor.start, corridor.end] {
            min_x = min_x.min(endpoint.x - half_w);
            max_x = max_x.max(endpoint.x + half_w);
            min_z = min_z.min(endpoint.z - half_w);
            max_z = max_z.max(endpoint.z + half_w);
        }
    }
    *bounds = FloorBounds {
        center: Vec3::new((min_x + max_x) * 0.5, 0.0, (min_z + max_z) * 0.5),
        half_x: (max_x - min_x) * 0.5,
        half_z: (max_z - min_z) * 0.5,
    };
    fog::spawn_fog_overlay(
        &mut commands,
        &mut meshes,
        &mut fog_materials,
        &mut images,
        bounds.center,
        bounds.half_x,
        bounds.half_z,
    );

    // Spawn enemies directly (FloorMap isn't available as Res yet during Startup)
    enemy::spawn_enemies_from_floor(&mut commands, &mut meshes, &mut materials, &floor_map);

    // Spawn bonfires in start rooms
    for placed in &floor_map.rooms {
        let template = &floor_map.templates[placed.template_index];
        if template.tag == RoomTag::Start {
            spawn_bonfire(&mut commands, &mut meshes, &mut materials, placed.world_center);
        }
    }

    // Lighting
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.72, 0.75, 0.84),
        brightness: 95.0,
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 22_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(-10.0, 18.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(floor_map);
}

pub const BONFIRE_INTERACT_RADIUS: f32 = 2.5;

#[derive(Component)]
pub struct Bonfire {
    pub lit: bool,
}

fn spawn_bonfire(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    center: Vec3,
) {
    let stone_color = Vec3::new(0.25, 0.24, 0.22);
    let ember_color = Vec3::new(0.90, 0.35, 0.08);
    let ash_color = Vec3::new(0.15, 0.13, 0.12);

    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(stone_color.x, stone_color.y, stone_color.z),
        perceptual_roughness: 1.0,
        ..default()
    });
    let ember = materials.add(StandardMaterial {
        base_color: Color::srgb(ember_color.x, ember_color.y, ember_color.z),
        emissive: LinearRgba::new(2.5, 0.8, 0.15, 1.0),
        perceptual_roughness: 0.7,
        ..default()
    });
    let ash = materials.add(StandardMaterial {
        base_color: Color::srgb(ash_color.x, ash_color.y, ash_color.z),
        perceptual_roughness: 1.0,
        ..default()
    });

    let ring_mesh = meshes.add(Cylinder::new(0.70, 0.25).mesh().resolution(8));
    let coal_mesh = meshes.add(Sphere::new(0.18).mesh().ico(1).unwrap());
    let log_mesh = meshes.add(Capsule3d::new(0.08, 0.50).mesh().longitudes(5).latitudes(4));
    let ember_mesh = meshes.add(Sphere::new(0.10).mesh().ico(1).unwrap());

    commands
        .spawn((
            Bonfire { lit: true },
            Transform::from_translation(center),
            Visibility::Visible,
        ))
        .with_children(|parent| {
            // Stone ring base
            parent.spawn((
                Mesh3d(ring_mesh),
                MeshMaterial3d(stone.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0),
            ));

            // Ash bed
            parent.spawn((
                Mesh3d(coal_mesh.clone()),
                MeshMaterial3d(ash),
                Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(1.8, 0.5, 1.8)),
            ));

            // Logs (crossed)
            for angle in [0.0_f32, 1.1, 2.3, 3.6] {
                let x = angle.cos() * 0.15;
                let z = angle.sin() * 0.15;
                parent.spawn((
                    Mesh3d(log_mesh.clone()),
                    MeshMaterial3d(stone.clone()),
                    Transform::from_xyz(x, 0.35, z)
                        .with_rotation(
                            Quat::from_rotation_y(angle) * Quat::from_rotation_z(0.45),
                        ),
                ));
            }

            // Glowing embers
            for (dx, dz) in [(0.0, 0.0), (0.12, 0.08), (-0.10, 0.06), (0.05, -0.11)] {
                parent.spawn((
                    Mesh3d(ember_mesh.clone()),
                    MeshMaterial3d(ember.clone()),
                    Transform::from_xyz(dx, 0.30, dz),
                ));
            }

            // Point light for the fire glow
            parent.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.6, 0.2),
                    intensity: 8_000.0,
                    range: 12.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.0, 0.0),
            ));
        });
}
