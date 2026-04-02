use std::collections::HashMap;

use bevy::prelude::*;

use crate::enemy::EnemyCollision;
use crate::player::{Player, PLAYER_COLLISION_RADIUS};

use super::fog::{FogMaterialSet, FogStatic};
use super::room::{DoorSide, RoomTemplate};

pub const TILE_SIZE: f32 = 2.0;
const TILE_HEIGHT: f32 = 0.24;
const ARENA_EDGE_PADDING: f32 = 0.05;

/// Marker for floor tile entities so they can be despawned on floor change.
#[derive(Component)]
pub struct FloorTile;

/// Marker for room-owned decorations (obelisks, etc.).
#[derive(Component)]
pub struct RoomDecor;

pub struct FloorSceneAssets {
    tile_mesh: Handle<Mesh>,
    border_material: FogMaterialSet,
    dark_material: FogMaterialSet,
    light_material: FogMaterialSet,
    corridor_material: FogMaterialSet,
    room_wall_material: FogMaterialSet,
    corridor_wall_material: FogMaterialSet,
    wall_mesh_x: Handle<Mesh>,
    wall_mesh_z: Handle<Mesh>,
    obelisk_mesh: Handle<Mesh>,
    obelisk_cap_mesh: Handle<Mesh>,
    obelisk_material: FogMaterialSet,
}

impl FloorSceneAssets {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            tile_mesh: meshes.add(Cuboid::new(TILE_SIZE, TILE_HEIGHT, TILE_SIZE)),
            border_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.13, 0.15, 0.18),
                StandardMaterial {
                    base_color: Color::srgb(0.13, 0.15, 0.18),
                    perceptual_roughness: 1.0,
                    ..default()
                },
            ),
            dark_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.22, 0.24, 0.28),
                StandardMaterial {
                    base_color: Color::srgb(0.22, 0.24, 0.28),
                    perceptual_roughness: 1.0,
                    ..default()
                },
            ),
            light_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.28, 0.31, 0.35),
                StandardMaterial {
                    base_color: Color::srgb(0.28, 0.31, 0.35),
                    perceptual_roughness: 0.96,
                    ..default()
                },
            ),
            corridor_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.18, 0.20, 0.24),
                StandardMaterial {
                    base_color: Color::srgb(0.18, 0.20, 0.24),
                    perceptual_roughness: 1.0,
                    ..default()
                },
            ),
            room_wall_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.16, 0.17, 0.20),
                StandardMaterial {
                    base_color: Color::srgb(0.16, 0.17, 0.20),
                    perceptual_roughness: 1.0,
                    ..default()
                },
            ),
            corridor_wall_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.14, 0.15, 0.18),
                StandardMaterial {
                    base_color: Color::srgb(0.14, 0.15, 0.18),
                    perceptual_roughness: 1.0,
                    ..default()
                },
            ),
            wall_mesh_x: meshes.add(Cuboid::new(TILE_SIZE, WALL_HEIGHT, WALL_THICKNESS)),
            wall_mesh_z: meshes.add(Cuboid::new(WALL_THICKNESS, WALL_HEIGHT, TILE_SIZE)),
            obelisk_mesh: meshes.add(Cuboid::new(1.0, 3.4, 1.0)),
            obelisk_cap_mesh: meshes.add(Cuboid::new(1.3, 0.5, 1.3)),
            obelisk_material: FogMaterialSet::new(
                materials,
                Vec3::new(0.30, 0.29, 0.33),
                StandardMaterial {
                    base_color: Color::srgb(0.30, 0.29, 0.33),
                    perceptual_roughness: 1.0,
                    ..default()
                },
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WallSegment {
    pub(crate) center: Vec2,
    pub(crate) half_extents: Vec2,
    pub(crate) blocks_los: bool,
}

#[derive(Resource)]
pub struct WallSpatialIndex {
    cell_size: f32,
    cells: HashMap<IVec2, Vec<usize>>,
    segments: Vec<WallSegment>,
}

impl Default for WallSpatialIndex {
    fn default() -> Self {
        Self {
            cell_size: TILE_SIZE * 2.0,
            cells: HashMap::new(),
            segments: Vec::new(),
        }
    }
}

impl WallSpatialIndex {
    pub fn rebuild(&mut self, segments: Vec<WallSegment>) {
        self.cells.clear();
        self.segments = segments;
        self.cells.reserve(self.segments.len());

        for (index, segment) in self.segments.iter().enumerate() {
            let min = segment.center - segment.half_extents;
            let max = segment.center + segment.half_extents;
            let (min_cell, max_cell) = self.cell_range(min, max);

            for x in min_cell.x..=max_cell.x {
                for z in min_cell.y..=max_cell.y {
                    self.cells.entry(IVec2::new(x, z)).or_default().push(index);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn segment_clear(&self, start: Vec2, end: Vec2, radius: f32) -> bool {
        self.first_hit_fraction(start, end, radius).is_none()
    }

    pub fn segment_clear_los(&self, start: Vec2, end: Vec2, radius: f32) -> bool {
        self.first_hit_fraction_los(start, end, radius).is_none()
    }

    pub fn first_hit_fraction_los(&self, start: Vec2, end: Vec2, radius: f32) -> Option<f32> {
        let min = start.min(end) - Vec2::splat(radius);
        let max = start.max(end) + Vec2::splat(radius);
        let (min_cell, max_cell) = self.cell_range(min, max);
        let mut best_hit: Option<f32> = None;

        for x in min_cell.x..=max_cell.x {
            for z in min_cell.y..=max_cell.y {
                let Some(indices) = self.cells.get(&IVec2::new(x, z)) else {
                    continue;
                };

                for &index in indices {
                    let segment = &self.segments[index];
                    if !segment.blocks_los {
                        continue;
                    }
                    let Some(hit_fraction) = segment_aabb_intersection_fraction(
                        start,
                        end,
                        segment.center,
                        segment.half_extents + Vec2::splat(radius),
                    ) else {
                        continue;
                    };

                    best_hit = Some(match best_hit {
                        Some(best) => best.min(hit_fraction),
                        None => hit_fraction,
                    });
                }
            }
        }

        best_hit
    }

    pub fn first_hit_fraction(&self, start: Vec2, end: Vec2, radius: f32) -> Option<f32> {
        let min = start.min(end) - Vec2::splat(radius);
        let max = start.max(end) + Vec2::splat(radius);
        let (min_cell, max_cell) = self.cell_range(min, max);
        let mut best_hit: Option<f32> = None;

        for x in min_cell.x..=max_cell.x {
            for z in min_cell.y..=max_cell.y {
                let Some(indices) = self.cells.get(&IVec2::new(x, z)) else {
                    continue;
                };

                for &index in indices {
                    let segment = &self.segments[index];
                    let Some(hit_fraction) = segment_aabb_intersection_fraction(
                        start,
                        end,
                        segment.center,
                        segment.half_extents + Vec2::splat(radius),
                    ) else {
                        continue;
                    };

                    best_hit = Some(match best_hit {
                        Some(best) => best.min(hit_fraction),
                        None => hit_fraction,
                    });
                }
            }
        }

        best_hit
    }

    pub fn for_each_nearby_segment(
        &self,
        point: Vec2,
        radius: f32,
        mut visit: impl FnMut(&WallSegment),
    ) {
        let min = point - Vec2::splat(radius);
        let max = point + Vec2::splat(radius);
        let (min_cell, max_cell) = self.cell_range(min, max);
        for x in min_cell.x..=max_cell.x {
            for z in min_cell.y..=max_cell.y {
                let Some(indices) = self.cells.get(&IVec2::new(x, z)) else {
                    continue;
                };

                for &index in indices {
                    visit(&self.segments[index]);
                }
            }
        }
    }

    fn cell_range(&self, min: Vec2, max: Vec2) -> (IVec2, IVec2) {
        (self.cell_coords(min), self.cell_coords(max))
    }

    fn cell_coords(&self, point: Vec2) -> IVec2 {
        IVec2::new(
            (point.x / self.cell_size).floor() as i32,
            (point.y / self.cell_size).floor() as i32,
        )
    }
}

/// Conservative floor-wide movement extents used as a broad-phase clamp.
/// Walls and line-of-sight helpers provide the fine-grained room/corridor limits.
#[derive(Resource)]
pub struct FloorBounds {
    /// World-space center of the overall floor AABB.
    pub center: Vec3,
    /// Half-extent in X from center.
    pub half_x: f32,
    /// Half-extent in Z from center.
    pub half_z: f32,
}

impl Default for FloorBounds {
    fn default() -> Self {
        // Fallback: large enough to not clip anything during startup.
        Self {
            center: Vec3::ZERO,
            half_x: 100.0,
            half_z: 100.0,
        }
    }
}

impl FloorBounds {
    #[cfg(test)]
    pub fn from_template(template: &RoomTemplate, center: Vec3) -> Self {
        Self {
            center,
            half_x: template.half_extent_x(),
            half_z: template.half_extent_z(),
        }
    }

    fn clamp_half_x(&self, radius: f32) -> f32 {
        (self.half_x - radius - ARENA_EDGE_PADDING).max(0.0)
    }

    fn clamp_half_z(&self, radius: f32) -> f32 {
        (self.half_z - radius - ARENA_EDGE_PADDING).max(0.0)
    }
}

/// Clamp a ground-plane movement target to the floor's broad-phase AABB.
pub fn clamp_ground_target(bounds: &FloorBounds, target: Vec2, radius: f32) -> Vec2 {
    let hx = bounds.clamp_half_x(radius);
    let hz = bounds.clamp_half_z(radius);
    let cx = bounds.center.x;
    let cz = bounds.center.z;
    Vec2::new(
        target.x.clamp(cx - hx, cx + hx),
        target.y.clamp(cz - hz, cz + hz),
    )
}

/// Clamp a translation in-place to the floor's broad-phase AABB.
pub fn clamp_translation(bounds: &FloorBounds, translation: &mut Vec3, radius: f32) {
    let hx = bounds.clamp_half_x(radius);
    let hz = bounds.clamp_half_z(radius);
    let cx = bounds.center.x;
    let cz = bounds.center.z;
    translation.x = translation.x.clamp(cx - hx, cx + hx);
    translation.z = translation.z.clamp(cz - hz, cz + hz);
}

#[allow(dead_code)]
pub fn sweep_ground_target<'a, I>(
    bounds: &FloorBounds,
    start: Vec2,
    desired_end: Vec2,
    radius: f32,
    walls: I,
    clearance: f32,
) -> Vec2
where
    I: IntoIterator<Item = (&'a Transform, &'a ObstacleCollider)>,
{
    let clamped_end = clamp_ground_target(bounds, desired_end, radius);
    clip_segment_to_walls(start, clamped_end, radius, walls, clearance)
}

pub fn sweep_ground_target_indexed(
    bounds: &FloorBounds,
    start: Vec2,
    desired_end: Vec2,
    radius: f32,
    wall_index: &WallSpatialIndex,
    clearance: f32,
) -> Vec2 {
    let clamped_end = clamp_ground_target(bounds, desired_end, radius);
    clip_segment_to_wall_index(start, clamped_end, radius, wall_index, clearance)
}

#[allow(dead_code)]
pub fn clip_segment_to_walls<'a, I>(
    start: Vec2,
    end: Vec2,
    radius: f32,
    walls: I,
    clearance: f32,
) -> Vec2
where
    I: IntoIterator<Item = (&'a Transform, &'a ObstacleCollider)>,
{
    let delta = end - start;
    let length = delta.length();
    if length <= 0.0001 {
        return end;
    }

    let Some(hit_fraction) = first_wall_hit_fraction(start, end, radius, walls) else {
        return end;
    };

    let backoff = (clearance / length).clamp(0.0, hit_fraction);
    start + delta * (hit_fraction - backoff).max(0.0)
}

pub fn clip_segment_to_wall_index(
    start: Vec2,
    end: Vec2,
    radius: f32,
    wall_index: &WallSpatialIndex,
    clearance: f32,
) -> Vec2 {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.0001 {
        return end;
    }

    let Some(hit_fraction) = wall_index.first_hit_fraction(start, end, radius) else {
        return end;
    };

    let backoff = (clearance / length).clamp(0.0, hit_fraction);
    start + delta * (hit_fraction - backoff).max(0.0)
}

#[allow(dead_code)]
pub fn segment_clear_of_walls<'a, I>(start: Vec2, end: Vec2, radius: f32, walls: I) -> bool
where
    I: IntoIterator<Item = (&'a Transform, &'a ObstacleCollider)>,
{
    first_wall_hit_fraction(start, end, radius, walls).is_none()
}

pub fn segment_clear_with_index(
    start: Vec2,
    end: Vec2,
    radius: f32,
    wall_index: &WallSpatialIndex,
) -> bool {
    wall_index.segment_clear_los(start, end, radius)
}

#[allow(dead_code)]
pub fn first_wall_hit_fraction<'a, I>(start: Vec2, end: Vec2, radius: f32, walls: I) -> Option<f32>
where
    I: IntoIterator<Item = (&'a Transform, &'a ObstacleCollider)>,
{
    let mut best_hit: Option<f32> = None;

    for (wall_tf, wall) in walls {
        let center = Vec2::new(wall_tf.translation.x, wall_tf.translation.z);
        let half_extents = Vec2::new(wall.half_x + radius, wall.half_z + radius);
        let Some(hit_fraction) =
            segment_aabb_intersection_fraction(start, end, center, half_extents)
        else {
            continue;
        };

        best_hit = Some(match best_hit {
            Some(best) => best.min(hit_fraction),
            None => hit_fraction,
        });
    }

    best_hit
}

pub fn segment_aabb_intersection_fraction(
    start: Vec2,
    end: Vec2,
    center: Vec2,
    half_extents: Vec2,
) -> Option<f32> {
    let min = center - half_extents;
    let max = center + half_extents;
    let delta = end - start;

    let mut t_min: f32 = 0.0;
    let mut t_max: f32 = 1.0;

    for axis in 0..2 {
        let start_axis = start[axis];
        let delta_axis = delta[axis];
        let min_axis = min[axis];
        let max_axis = max[axis];

        if delta_axis.abs() <= 0.0001 {
            if start_axis < min_axis || start_axis > max_axis {
                return None;
            }
            continue;
        }

        let inv_delta = 1.0 / delta_axis;
        let mut t1 = (min_axis - start_axis) * inv_delta;
        let mut t2 = (max_axis - start_axis) * inv_delta;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }

        t_min = t_min.max(t1);
        t_max = t_max.min(t2);
        if t_min > t_max {
            return None;
        }
    }

    Some(t_min.clamp(0.0, 1.0))
}

/// Convert a grid-local coordinate to world space given a room center.
pub fn grid_to_world(room_center: Vec3, template: &RoomTemplate, grid_x: i32, grid_z: i32) -> Vec3 {
    room_center + template.grid_to_local(grid_x, grid_z)
}

/// Spawn the floor tiles for a single room at the given world-space center.
pub fn spawn_room_floor(
    commands: &mut Commands,
    scene_assets: &FloorSceneAssets,
    template: &RoomTemplate,
    center: Vec3,
) {
    let w = template.width;
    let h = template.height;

    for gx in 0..w {
        for gz in 0..h {
            let is_border = gx == 0 || gz == 0 || gx == w - 1 || gz == h - 1;
            let material = if is_border {
                scene_assets.border_material.clone()
            } else if (gx + gz) % 2 == 0 {
                scene_assets.dark_material.clone()
            } else {
                scene_assets.light_material.clone()
            };

            let world_pos = grid_to_world(center, template, gx, gz);
            commands.spawn((
                FloorTile,
                Mesh3d(scene_assets.tile_mesh.clone()),
                MeshMaterial3d(material.visible.clone()),
                FogStatic::point(Vec2::new(world_pos.x, world_pos.z), material),
                Transform::from_translation(world_pos + Vec3::Y * (-TILE_HEIGHT * 0.5)),
            ));
        }
    }

    if template.obelisks {
        spawn_obelisks(commands, scene_assets, template, center);
    }
}

fn spawn_obelisks(
    commands: &mut Commands,
    scene_assets: &FloorSceneAssets,
    template: &RoomTemplate,
    center: Vec3,
) {
    let inset = 2;
    let w = template.width;
    let h = template.height;

    let corners = [
        grid_to_world(center, template, inset, inset),
        grid_to_world(center, template, inset, h - 1 - inset),
        grid_to_world(center, template, w - 1 - inset, inset),
        grid_to_world(center, template, w - 1 - inset, h - 1 - inset),
    ];

    for corner in corners {
        commands.spawn((
            RoomDecor,
            Mesh3d(scene_assets.obelisk_mesh.clone()),
            MeshMaterial3d(scene_assets.obelisk_material.visible.clone()),
            FogStatic::point(
                Vec2::new(corner.x, corner.z),
                scene_assets.obelisk_material.clone(),
            ),
            Transform::from_translation(corner + Vec3::new(0.0, 1.7, 0.0)),
        ));
        commands.spawn((
            RoomDecor,
            Mesh3d(scene_assets.obelisk_cap_mesh.clone()),
            MeshMaterial3d(scene_assets.obelisk_material.visible.clone()),
            FogStatic::point(
                Vec2::new(corner.x, corner.z),
                scene_assets.obelisk_material.clone(),
            ),
            Transform::from_translation(corner + Vec3::new(0.0, 3.65, 0.0)),
        ));
    }
}

const WALL_HEIGHT: f32 = 2.4;
const WALL_THICKNESS: f32 = 0.3;

/// Marker for wall entities.
#[derive(Component)]
pub struct Obstacle;

/// Axis-aligned collision box for a wall segment (XZ plane).
#[allow(dead_code)]
#[derive(Component)]
pub struct ObstacleCollider {
    pub half_x: f32,
    pub half_z: f32,
}

type WallCollisionPlayers<'w, 's> =
    Query<'w, 's, &'static mut Transform, (With<Player>, Without<Obstacle>, Without<EnemyCollision>)>;

type WallCollisionEnemies<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static EnemyCollision),
    (Without<Player>, Without<Obstacle>),
>;

/// Spawn walls around the perimeter of a room, with gaps only at doors that
/// have a corridor connecting to them.  `connected_sides` lists the
/// [`DoorSide`]s that are actually in use.
pub fn spawn_room_walls(
    commands: &mut Commands,
    scene_assets: &FloorSceneAssets,
    template: &RoomTemplate,
    center: Vec3,
    connected_sides: &[DoorSide],
    wall_segments: &mut Vec<WallSegment>,
) {
    let w = template.width;
    let h = template.height;

    // North and South walls (run along X)
    for gx in 0..w {
        // North wall (gz = h-1)
        if !is_connected_door_gap(template, gx, h - 1, connected_sides) {
            let pos = grid_to_world(center, template, gx, h - 1);
            spawn_wall_segment(
                commands,
                &scene_assets.wall_mesh_x,
                &scene_assets.room_wall_material,
                pos + Vec3::Z * (TILE_SIZE * 0.5),
                true,
                wall_segments,
            );
        }
        // South wall (gz = 0)
        if !is_connected_door_gap(template, gx, 0, connected_sides) {
            let pos = grid_to_world(center, template, gx, 0);
            spawn_wall_segment(
                commands,
                &scene_assets.wall_mesh_x,
                &scene_assets.room_wall_material,
                pos - Vec3::Z * (TILE_SIZE * 0.5),
                true,
                wall_segments,
            );
        }
    }

    // East and West walls (run along Z)
    for gz in 0..h {
        // East wall (gx = w-1)
        if !is_connected_door_gap(template, w - 1, gz, connected_sides) {
            let pos = grid_to_world(center, template, w - 1, gz);
            spawn_wall_segment(
                commands,
                &scene_assets.wall_mesh_z,
                &scene_assets.room_wall_material,
                pos + Vec3::X * (TILE_SIZE * 0.5),
                false,
                wall_segments,
            );
        }
        // West wall (gx = 0)
        if !is_connected_door_gap(template, 0, gz, connected_sides) {
            let pos = grid_to_world(center, template, 0, gz);
            spawn_wall_segment(
                commands,
                &scene_assets.wall_mesh_z,
                &scene_assets.room_wall_material,
                pos - Vec3::X * (TILE_SIZE * 0.5),
                false,
                wall_segments,
            );
        }
    }
}

/// Returns whether a border tile belongs to a connected door opening.
fn is_connected_door_gap(
    template: &RoomTemplate,
    gx: i32,
    gz: i32,
    connected_sides: &[DoorSide],
) -> bool {
    let w = template.width;
    let h = template.height;
    for door in &template.doors {
        if !connected_sides.contains(&door.side) {
            continue;
        }
        let axis_range = door.axis_range();
        match door.side {
            DoorSide::North if gz == h - 1 => {
                if axis_range.contains(&gx) {
                    return true;
                }
            }
            DoorSide::South if gz == 0 => {
                if axis_range.contains(&gx) {
                    return true;
                }
            }
            DoorSide::East if gx == w - 1 => {
                if axis_range.contains(&gz) {
                    return true;
                }
            }
            DoorSide::West if gx == 0 => {
                if axis_range.contains(&gz) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn spawn_wall_segment(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    material: &FogMaterialSet,
    pos: Vec3,
    along_x: bool,
    wall_segments: &mut Vec<WallSegment>,
) {
    let (half_x, half_z) = if along_x {
        (TILE_SIZE * 0.5, WALL_THICKNESS * 0.5)
    } else {
        (WALL_THICKNESS * 0.5, TILE_SIZE * 0.5)
    };
    wall_segments.push(WallSegment {
        center: Vec2::new(pos.x, pos.z),
        half_extents: Vec2::new(half_x, half_z),
        blocks_los: true,
    });
    commands.spawn((
        Obstacle,
        ObstacleCollider { half_x, half_z },
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.visible.clone()),
        FogStatic::edge(
            if along_x {
                Vec2::new(pos.x, pos.z - TILE_SIZE * 0.55)
            } else {
                Vec2::new(pos.x - TILE_SIZE * 0.55, pos.z)
            },
            if along_x {
                Vec2::new(pos.x, pos.z + TILE_SIZE * 0.55)
            } else {
                Vec2::new(pos.x + TILE_SIZE * 0.55, pos.z)
            },
            material.clone(),
        ),
        Transform::from_translation(Vec3::new(pos.x, WALL_HEIGHT * 0.5, pos.z)),
    ));
}

/// Spawn a corridor floor between two world-space endpoints.
pub fn spawn_corridor_floor(
    commands: &mut Commands,
    scene_assets: &FloorSceneAssets,
    start: Vec3,
    end: Vec3,
    width_tiles: i32,
    wall_segments: &mut Vec<WallSegment>,
) {
    let diff = end - start;
    let primarily_x = diff.x.abs() > diff.z.abs();

    if primarily_x {
        let step = TILE_SIZE * diff.x.signum();
        let count = (diff.x.abs() / TILE_SIZE).ceil() as i32;
        let half_w = width_tiles / 2;
        for i in 0..=count {
            let x = start.x + i as f32 * step;
            for w in -half_w..=half_w {
                let z = start.z + w as f32 * TILE_SIZE;
                commands.spawn((
                    FloorTile,
                    Mesh3d(scene_assets.tile_mesh.clone()),
                    MeshMaterial3d(scene_assets.corridor_material.visible.clone()),
                    FogStatic::point(Vec2::new(x, z), scene_assets.corridor_material.clone()),
                    Transform::from_translation(Vec3::new(x, -TILE_HEIGHT * 0.5, z)),
                ));
            }
            // Corridor walls on the sides — skip the tile right at each room
            // edge (i=0 / i=count) so it doesn't poke into the room; the next
            // tile lines up flush with the room's wall line.
            if i >= 1 && i < count {
                let wall_z_neg = start.z - (half_w as f32 + 0.5) * TILE_SIZE;
                let wall_z_pos = start.z + (half_w as f32 + 0.5) * TILE_SIZE;
                spawn_wall_segment(
                    commands,
                    &scene_assets.wall_mesh_x,
                    &scene_assets.corridor_wall_material,
                    Vec3::new(x, 0.0, wall_z_neg),
                    true,
                    wall_segments,
                );
                spawn_wall_segment(
                    commands,
                    &scene_assets.wall_mesh_x,
                    &scene_assets.corridor_wall_material,
                    Vec3::new(x, 0.0, wall_z_pos),
                    true,
                    wall_segments,
                );
            }
        }
    } else {
        let step = TILE_SIZE * diff.z.signum();
        let count = (diff.z.abs() / TILE_SIZE).ceil() as i32;
        let half_w = width_tiles / 2;
        for i in 0..=count {
            let z = start.z + i as f32 * step;
            for w in -half_w..=half_w {
                let x = start.x + w as f32 * TILE_SIZE;
                commands.spawn((
                    FloorTile,
                    Mesh3d(scene_assets.tile_mesh.clone()),
                    MeshMaterial3d(scene_assets.corridor_material.visible.clone()),
                    FogStatic::point(Vec2::new(x, z), scene_assets.corridor_material.clone()),
                    Transform::from_translation(Vec3::new(x, -TILE_HEIGHT * 0.5, z)),
                ));
            }
            // Corridor walls — same skip logic as the primarily-X branch.
            if i >= 1 && i < count {
                let wall_x_neg = start.x - (half_w as f32 + 0.5) * TILE_SIZE;
                let wall_x_pos = start.x + (half_w as f32 + 0.5) * TILE_SIZE;
                spawn_wall_segment(
                    commands,
                    &scene_assets.wall_mesh_z,
                    &scene_assets.corridor_wall_material,
                    Vec3::new(wall_x_neg, 0.0, z),
                    false,
                    wall_segments,
                );
                spawn_wall_segment(
                    commands,
                    &scene_assets.wall_mesh_z,
                    &scene_assets.corridor_wall_material,
                    Vec3::new(wall_x_pos, 0.0, z),
                    false,
                    wall_segments,
                );
            }
        }
    }
}

/// Push characters out of wall AABBs after all movement has finished.
pub fn resolve_wall_collisions(
    bounds: Res<FloorBounds>,
    wall_index: Res<WallSpatialIndex>,
    mut players: WallCollisionPlayers<'_, '_>,
    mut enemies: WallCollisionEnemies<'_, '_>,
) {
    for mut player_tf in &mut players {
        push_out_of_nearby_walls(
            &mut player_tf.translation,
            PLAYER_COLLISION_RADIUS,
            &wall_index,
        );
        clamp_translation(&bounds, &mut player_tf.translation, PLAYER_COLLISION_RADIUS);
    }
    for (mut enemy_tf, enemy_col) in &mut enemies {
        push_out_of_nearby_walls(&mut enemy_tf.translation, enemy_col.radius, &wall_index);
        clamp_translation(&bounds, &mut enemy_tf.translation, enemy_col.radius);
    }
}

#[allow(dead_code)]
fn push_out_of_obstacle(translation: &mut Vec3, radius: f32, obstacle_pos: Vec3, obstacle: &ObstacleCollider) {
    push_out_of_wall_segment(
        translation,
        radius,
        Vec2::new(obstacle_pos.x, obstacle_pos.z),
        Vec2::new(obstacle.half_x, obstacle.half_z),
    );
}

fn push_out_of_nearby_walls(translation: &mut Vec3, radius: f32, wall_index: &WallSpatialIndex) {
    for _ in 0..3 {
        let before = *translation;
        let center = Vec2::new(translation.x, translation.z);
        wall_index.for_each_nearby_segment(center, radius, |segment| {
            push_out_of_wall_segment(translation, radius, segment.center, segment.half_extents);
        });

        if translation.distance_squared(before) <= 0.000_001 {
            break;
        }
    }
}

fn push_out_of_wall_segment(
    translation: &mut Vec3,
    radius: f32,
    wall_center: Vec2,
    wall_half_extents: Vec2,
) {
    // Quick reject: skip walls clearly outside the actor's reach
    if (translation.x - wall_center.x).abs() > wall_half_extents.x + radius
        || (translation.z - wall_center.y).abs() > wall_half_extents.y + radius
    {
        return;
    }

    // Closest point on the wall AABB to the character center (XZ plane)
    let closest_x = translation.x.clamp(
        wall_center.x - wall_half_extents.x,
        wall_center.x + wall_half_extents.x,
    );
    let closest_z = translation.z.clamp(
        wall_center.y - wall_half_extents.y,
        wall_center.y + wall_half_extents.y,
    );

    let dx = translation.x - closest_x;
    let dz = translation.z - closest_z;
    let dist_sq = dx * dx + dz * dz;

    if dist_sq >= radius * radius {
        return;
    }

    if dist_sq > 0.0001 {
        // Center outside AABB but within radius — push along the separation vector
        let dist = dist_sq.sqrt();
        let penetration = radius - dist;
        translation.x += (dx / dist) * penetration;
        translation.z += (dz / dist) * penetration;
    } else {
        // Center inside the AABB — push out along shortest escape axis
        let pen_px = (wall_center.x + wall_half_extents.x + radius) - translation.x;
        let pen_nx = translation.x - (wall_center.x - wall_half_extents.x - radius);
        let pen_pz = (wall_center.y + wall_half_extents.y + radius) - translation.z;
        let pen_nz = translation.z - (wall_center.y - wall_half_extents.y - radius);
        let min = pen_px.min(pen_nx).min(pen_pz).min(pen_nz);
        if min == pen_px {
            translation.x = wall_center.x + wall_half_extents.x + radius;
        } else if min == pen_nx {
            translation.x = wall_center.x - wall_half_extents.x - radius;
        } else if min == pen_pz {
            translation.z = wall_center.y + wall_half_extents.y + radius;
        } else {
            translation.z = wall_center.y - wall_half_extents.y - radius;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::room::built_in_templates;
    use super::*;

    #[test]
    fn clamp_respects_room_bounds() {
        let templates = built_in_templates();
        let template = &templates[3]; // Grand Arena 20x20
        let bounds = FloorBounds::from_template(template, Vec3::ZERO);
        let clamped = clamp_ground_target(&bounds, Vec2::new(999.0, -999.0), 0.75);
        let hx = bounds.clamp_half_x(0.75);
        let hz = bounds.clamp_half_z(0.75);
        assert_eq!(clamped, Vec2::new(hx, -hz));
    }

    #[test]
    fn clamp_respects_offset_center() {
        let templates = built_in_templates();
        let template = &templates[0]; // Start Chamber 12x12
        let center = Vec3::new(100.0, 0.0, 50.0);
        let bounds = FloorBounds::from_template(template, center);
        let clamped = clamp_ground_target(&bounds, Vec2::new(999.0, -999.0), 0.5);
        let hx = bounds.clamp_half_x(0.5);
        let hz = bounds.clamp_half_z(0.5);
        assert_eq!(clamped, Vec2::new(center.x + hx, center.z - hz));
    }

    #[test]
    fn segment_aabb_intersection_returns_first_hit_fraction() {
        let hit = segment_aabb_intersection_fraction(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(1.0, 1.0),
        );

        assert_eq!(hit, Some(0.4));
    }

    #[test]
    fn segment_aabb_intersection_ignores_clear_paths() {
        let hit = segment_aabb_intersection_fraction(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, 3.0),
            Vec2::new(1.0, 1.0),
        );

        assert_eq!(hit, None);
    }

    #[test]
    fn clip_segment_to_walls_respects_clearance() {
        let wall_tf = Transform::from_xyz(5.0, 0.0, 0.0);
        let wall = ObstacleCollider {
            half_x: 1.0,
            half_z: 1.0,
        };

        let clipped = clip_segment_to_walls(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            0.0,
            [(&wall_tf, &wall)],
            0.5,
        );

        assert_eq!(clipped, Vec2::new(3.5, 0.0));
    }

    #[test]
    fn push_out_of_wall_resolves_embedded_centers() {
        let wall = ObstacleCollider {
            half_x: 1.0,
            half_z: 1.0,
        };
        let mut translation = Vec3::new(0.25, 0.0, 0.0);

        push_out_of_obstacle(&mut translation, 0.75, Vec3::ZERO, &wall);

        assert!(translation.x >= wall.half_x + 0.75);
        assert_eq!(translation.z, 0.0);
    }

    #[test]
    fn wall_spatial_index_matches_direct_hit_tests() {
        let mut index = WallSpatialIndex::default();
        index.rebuild(vec![
            WallSegment {
                center: Vec2::new(5.0, 0.0),
                half_extents: Vec2::new(1.0, 1.0),
                blocks_los: true,
            },
            WallSegment {
                center: Vec2::new(12.0, 3.0),
                half_extents: Vec2::new(1.0, 1.0),
                blocks_los: true,
            },
        ]);
        let wall_a_tf = Transform::from_xyz(5.0, 0.0, 0.0);
        let wall_b_tf = Transform::from_xyz(12.0, 0.0, 3.0);
        let wall_a = ObstacleCollider {
            half_x: 1.0,
            half_z: 1.0,
        };
        let wall_b = ObstacleCollider {
            half_x: 1.0,
            half_z: 1.0,
        };

        let direct_hit = first_wall_hit_fraction(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            0.0,
            [(&wall_a_tf, &wall_a), (&wall_b_tf, &wall_b)],
        );

        let indexed_hit = index.first_hit_fraction(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), 0.0);

        assert_eq!(indexed_hit, direct_hit);
    }
}
