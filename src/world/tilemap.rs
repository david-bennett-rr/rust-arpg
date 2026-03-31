use bevy::prelude::*;

pub const MAP_SIZE: i32 = 20;
pub const TILE_SIZE: f32 = 2.0;
const TILE_HEIGHT: f32 = 0.24;

#[derive(Component)]
pub struct FloorTile;

pub fn grid_to_world(grid_x: i32, grid_z: i32) -> Vec3 {
    let half_extent = (MAP_SIZE as f32 - 1.0) * TILE_SIZE * 0.5;
    Vec3::new(
        grid_x as f32 * TILE_SIZE - half_extent,
        0.0,
        grid_z as f32 * TILE_SIZE - half_extent,
    )
}

pub fn spawn_test_floor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.72, 0.75, 0.84),
        brightness: 95.0,
        ..default()
    });

    let tile_mesh = meshes.add(Cuboid::new(TILE_SIZE, TILE_HEIGHT, TILE_SIZE));
    let border_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.13, 0.15, 0.18),
        perceptual_roughness: 1.0,
        ..default()
    });
    let dark_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.24, 0.28),
        perceptual_roughness: 1.0,
        ..default()
    });
    let light_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.31, 0.35),
        perceptual_roughness: 0.96,
        ..default()
    });
    let stone_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.29, 0.33),
        perceptual_roughness: 1.0,
        ..default()
    });

    for grid_x in 0..MAP_SIZE {
        for grid_z in 0..MAP_SIZE {
            let material =
                if grid_x == 0 || grid_z == 0 || grid_x == MAP_SIZE - 1 || grid_z == MAP_SIZE - 1 {
                    border_material.clone()
                } else if (grid_x + grid_z) % 2 == 0 {
                    dark_material.clone()
                } else {
                    light_material.clone()
                };

            commands.spawn((
                FloorTile,
                Mesh3d(tile_mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(
                    grid_to_world(grid_x, grid_z) + Vec3::Y * (-TILE_HEIGHT * 0.5),
                ),
            ));
        }
    }

    spawn_corner_obelisks(&mut commands, &mut meshes, stone_material);

    commands.spawn((
        DirectionalLight {
            illuminance: 22_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(-10.0, 18.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_corner_obelisks(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    stone_material: Handle<StandardMaterial>,
) {
    let obelisk_mesh = meshes.add(Cuboid::new(1.0, 3.4, 1.0));
    let cap_mesh = meshes.add(Cuboid::new(1.3, 0.5, 1.3));
    let inset = 2;
    let corners = [
        grid_to_world(inset, inset),
        grid_to_world(inset, MAP_SIZE - 1 - inset),
        grid_to_world(MAP_SIZE - 1 - inset, inset),
        grid_to_world(MAP_SIZE - 1 - inset, MAP_SIZE - 1 - inset),
    ];

    for corner in corners {
        commands.spawn((
            Mesh3d(obelisk_mesh.clone()),
            MeshMaterial3d(stone_material.clone()),
            Transform::from_translation(corner + Vec3::new(0.0, 1.7, 0.0)),
        ));
        commands.spawn((
            Mesh3d(cap_mesh.clone()),
            MeshMaterial3d(stone_material.clone()),
            Transform::from_translation(corner + Vec3::new(0.0, 3.65, 0.0)),
        ));
    }
}
