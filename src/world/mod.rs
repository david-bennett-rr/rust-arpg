pub mod floor;
pub mod fog;
pub mod room;
pub mod tilemap;

use bevy::{pbr::MaterialPlugin, prelude::*};

use crate::enemy;
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
