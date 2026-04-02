use bevy::prelude::*;

use crate::rng::SplitMix64;

use super::room::{built_in_templates, DoorSide, RoomTag, RoomTemplate};

/// A placed room in the floor layout.
#[derive(Debug, Clone)]
pub struct PlacedRoom {
    /// Index into the template library.
    pub template_index: usize,
    /// Center of this room in world space.
    pub world_center: Vec3,
    /// Which coarse-grid cell this room occupies.
    pub grid_cell: (i32, i32),
    /// Connections to other rooms: (neighbor PlacedRoom index, our door side).
    pub connections: Vec<(usize, DoorSide)>,
}

/// A corridor segment connecting two rooms.
#[derive(Debug, Clone)]
pub struct Corridor {
    /// World-space start/end of the corridor centerline.
    pub start: Vec3,
    pub end: Vec3,
    /// Corridor width in tiles.
    pub width: i32,
}

/// The complete layout for one dungeon floor.
#[derive(Resource, Debug, Clone)]
pub struct FloorMap {
    pub rooms: Vec<PlacedRoom>,
    pub corridors: Vec<Corridor>,
    pub templates: Vec<RoomTemplate>,
    pub start_room: usize,
}

/// Spacing between room centers on the coarse grid (world units).
const COARSE_CELL_SIZE: f32 = 56.0;

/// Pick a template that has a door on `entry_side`.  Falls back to any
/// candidate if none match (shouldn't happen with well-defined templates).
fn pick_template_with_door(
    templates: &[RoomTemplate],
    candidates: &[usize],
    entry_side: DoorSide,
    rng: &mut SplitMix64,
) -> usize {
    if candidates.is_empty() {
        return rng.next_usize(templates.len());
    }
    let valid: Vec<usize> = candidates
        .iter()
        .filter(|&&ti| templates[ti].doors.iter().any(|d| d.side == entry_side))
        .copied()
        .collect();
    if valid.is_empty() {
        candidates[rng.next_usize(candidates.len())]
    } else {
        valid[rng.next_usize(valid.len())]
    }
}

/// Generate a floor layout.
///
/// The algorithm:
/// 1. Place a start room at (0,0) on a coarse grid.
/// 2. Random-walk outward, placing combat rooms at each step.
/// 3. The farthest room from start becomes the boss room.
/// 4. Branch 1-2 side rooms off the critical path.
pub fn generate_floor(seed: u64) -> FloorMap {
    let templates = built_in_templates();
    let mut rng = SplitMix64::new(seed);

    // Find template indices by tag
    let start_templates: Vec<usize> = templates
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tag == RoomTag::Start)
        .map(|(i, _)| i)
        .collect();
    let combat_templates: Vec<usize> = templates
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tag == RoomTag::Combat)
        .map(|(i, _)| i)
        .collect();
    let elite_templates: Vec<usize> = templates
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tag == RoomTag::Elite)
        .map(|(i, _)| i)
        .collect();
    let boss_templates: Vec<usize> = templates
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tag == RoomTag::Boss)
        .map(|(i, _)| i)
        .collect();

    let mut rooms: Vec<PlacedRoom> = Vec::new();
    let mut occupied: Vec<(i32, i32)> = Vec::new();

    // 1. Place start room at (0, 0)
    let start_ti = if start_templates.is_empty() {
        0
    } else {
        start_templates[rng.next_usize(start_templates.len())]
    };
    rooms.push(PlacedRoom {
        template_index: start_ti,
        world_center: Vec3::ZERO,
        grid_cell: (0, 0),
        connections: Vec::new(),
    });
    occupied.push((0, 0));

    // 2. Critical path: walk 4-5 rooms in random directions
    let path_length = 4 + rng.next_usize(2); // 4 or 5
    let directions = [
        (DoorSide::North, (0, 1)),
        (DoorSide::South, (0, -1)),
        (DoorSide::East, (1, 0)),
        (DoorSide::West, (-1, 0)),
    ];

    let mut current_cell = (0_i32, 0_i32);
    let mut current_room_idx = 0;
    let mut last_direction: Option<DoorSide> = None;

    for step in 0..path_length {
        // Pick a direction where the current room has a door on that side,
        // doesn't go to an occupied cell, and doesn't immediately reverse
        let current_template = &templates[rooms[current_room_idx].template_index];
        let candidates: Vec<(DoorSide, (i32, i32))> = directions
            .iter()
            .filter(|(side, (dx, dz))| {
                let has_exit_door = current_template.doors.iter().any(|d| d.side == *side);
                let new_cell = (current_cell.0 + dx, current_cell.1 + dz);
                has_exit_door
                    && !occupied.contains(&new_cell)
                    && last_direction.is_none_or(|last| *side != last.opposite())
            })
            .copied()
            .collect();

        if candidates.is_empty() {
            break;
        }

        let choice = rng.next_usize(candidates.len());
        let (side, (dx, dz)) = candidates[choice];
        let new_cell = (current_cell.0 + dx, current_cell.1 + dz);
        let world_pos = Vec3::new(
            new_cell.0 as f32 * COARSE_CELL_SIZE,
            0.0,
            new_cell.1 as f32 * COARSE_CELL_SIZE,
        );

        // Pick template: last step = boss, second to last = elite, rest = combat.
        // The new room must have a door on the entry side.
        let entry_side = side.opposite();
        let ti = if step == path_length - 1 {
            pick_template_with_door(&templates, &boss_templates, entry_side, &mut rng)
        } else if step == path_length - 2 && !elite_templates.is_empty() {
            pick_template_with_door(&templates, &elite_templates, entry_side, &mut rng)
        } else {
            pick_template_with_door(&templates, &combat_templates, entry_side, &mut rng)
        };

        let new_room_idx = rooms.len();
        rooms.push(PlacedRoom {
            template_index: ti,
            world_center: world_pos,
            grid_cell: new_cell,
            connections: Vec::new(),
        });
        occupied.push(new_cell);

        // Connect both directions
        rooms[current_room_idx]
            .connections
            .push((new_room_idx, side));
        rooms[new_room_idx]
            .connections
            .push((current_room_idx, side.opposite()));

        current_cell = new_cell;
        current_room_idx = new_room_idx;
        last_direction = Some(side);
    }

    // 3. Branch 1-2 side rooms off the critical path
    let critical_path_room_count = rooms.len();
    let branch_count = 1 + rng.next_usize(2);
    for _ in 0..branch_count {
        // Pick a random room on the path (not start or boss) to branch from
        if critical_path_room_count < 3 {
            break;
        }
        let Some(branch_from) = pick_branch_source_index(&mut rng, critical_path_room_count) else {
            break;
        };
        let branch_cell = rooms[branch_from].grid_cell;

        let branch_template = &templates[rooms[branch_from].template_index];
        let candidates: Vec<(DoorSide, (i32, i32))> = directions
            .iter()
            .filter(|(side, (dx, dz))| {
                let has_exit_door = branch_template.doors.iter().any(|d| d.side == *side);
                let new_cell = (branch_cell.0 + dx, branch_cell.1 + dz);
                has_exit_door && !occupied.contains(&new_cell)
            })
            .copied()
            .collect();

        if candidates.is_empty() {
            continue;
        }

        let choice = rng.next_usize(candidates.len());
        let (side, (dx, dz)) = candidates[choice];
        let new_cell = (branch_cell.0 + dx, branch_cell.1 + dz);
        let world_pos = Vec3::new(
            new_cell.0 as f32 * COARSE_CELL_SIZE,
            0.0,
            new_cell.1 as f32 * COARSE_CELL_SIZE,
        );

        let entry_side = side.opposite();
        let ti = pick_template_with_door(&templates, &combat_templates, entry_side, &mut rng);
        let new_room_idx = rooms.len();
        rooms.push(PlacedRoom {
            template_index: ti,
            world_center: world_pos,
            grid_cell: new_cell,
            connections: Vec::new(),
        });
        occupied.push(new_cell);

        rooms[branch_from].connections.push((new_room_idx, side));
        rooms[new_room_idx]
            .connections
            .push((branch_from, side.opposite()));
    }

    // 4. Build corridors
    let mut corridors = Vec::new();
    let mut seen_pairs = Vec::new();
    for (room_idx, room) in rooms.iter().enumerate() {
        for &(neighbor_idx, side) in &room.connections {
            let pair = if room_idx < neighbor_idx {
                (room_idx, neighbor_idx)
            } else {
                (neighbor_idx, room_idx)
            };
            if seen_pairs.contains(&pair) {
                continue;
            }
            seen_pairs.push(pair);

            let template_a = &templates[room.template_index];
            let template_b = &templates[rooms[neighbor_idx].template_index];

            // Find the matching door on each room
            let door_a = template_a
                .doors
                .iter()
                .find(|d| d.side == side)
                .or_else(|| template_a.doors.first());
            let neighbor_side = side.opposite();
            let door_b = template_b
                .doors
                .iter()
                .find(|d| d.side == neighbor_side)
                .or_else(|| template_b.doors.first());

            let start = if let Some(door) = door_a {
                room.world_center + template_a.door_world_offset(door)
            } else {
                room.world_center
            };
            let end = if let Some(door) = door_b {
                rooms[neighbor_idx].world_center + template_b.door_world_offset(door)
            } else {
                rooms[neighbor_idx].world_center
            };

            corridors.push(Corridor {
                start,
                end,
                width: 3,
            });
        }
    }

    FloorMap {
        rooms,
        corridors,
        templates,
        start_room: 0,
    }
}

fn pick_branch_source_index(
    rng: &mut SplitMix64,
    critical_path_room_count: usize,
) -> Option<usize> {
    if critical_path_room_count < 3 {
        return None;
    }

    Some(1 + rng.next_usize(critical_path_room_count - 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_sources_stay_on_critical_path_interior() {
        let mut rng = SplitMix64::new(123);

        for _ in 0..128 {
            let source = pick_branch_source_index(&mut rng, 6).unwrap();
            assert!((1..5).contains(&source));
        }
    }

    #[test]
    fn short_critical_paths_have_no_branch_source() {
        let mut rng = SplitMix64::new(123);

        assert_eq!(pick_branch_source_index(&mut rng, 0), None);
        assert_eq!(pick_branch_source_index(&mut rng, 1), None);
        assert_eq!(pick_branch_source_index(&mut rng, 2), None);
    }
}
