use bevy::prelude::*;

/// Cardinal direction for a door on a room edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DoorSide {
    North,
    South,
    East,
    West,
}

impl DoorSide {
    pub fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::South => Self::North,
            Self::East => Self::West,
            Self::West => Self::East,
        }
    }
}

/// A door opening on the edge of a room template.
#[derive(Debug, Clone)]
pub struct DoorSlot {
    /// Which edge the door is on.
    pub side: DoorSide,
    /// Position along that edge in grid coords (centered on the room).
    /// For North/South doors this is the X grid position.
    /// For East/West doors this is the Z grid position.
    /// Even widths use this as the positive-side center tile, so the true
    /// opening center sits half a tile toward negative X/Z.
    pub position: i32,
    /// Width of the opening in tiles.
    pub width: i32,
}

impl DoorSlot {
    pub(crate) fn axis_range(&self) -> std::ops::Range<i32> {
        let start = self.position - self.width / 2;
        start..start + self.width.max(0)
    }

    fn axis_center(&self) -> f32 {
        let even_width_offset = if self.width % 2 == 0 { -0.5 } else { 0.0 };
        self.position as f32 + even_width_offset
    }
}

/// What kind of enemy can spawn in this room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemySlot {
    Rat,
    Goblin,
}

/// A spawn point for an enemy within a room template.
#[derive(Debug, Clone)]
pub struct EnemySpawn {
    pub kind: EnemySlot,
    /// Grid-local position within the room.
    pub grid_x: i32,
    pub grid_z: i32,
}

/// Tag describing room purpose in the floor graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomTag {
    Start,
    Combat,
    Elite,
    Boss,
}

/// A room template: tile dimensions, doors, enemy spawns.
#[derive(Debug, Clone)]
pub struct RoomTemplate {
    pub width: i32,
    pub height: i32,
    pub doors: Vec<DoorSlot>,
    pub enemies: Vec<EnemySpawn>,
    pub tag: RoomTag,
    /// Whether to spawn corner obelisks.
    pub obelisks: bool,
}

impl RoomTemplate {
    pub fn tile_size() -> f32 {
        2.0
    }

    /// Half-extent of this room in world units, measured from center.
    pub fn half_extent_x(&self) -> f32 {
        (self.width as f32 - 1.0) * Self::tile_size() * 0.5
    }

    pub fn half_extent_z(&self) -> f32 {
        (self.height as f32 - 1.0) * Self::tile_size() * 0.5
    }

    /// Convert a grid-local coordinate to a world-space offset from the room center.
    pub fn grid_to_local(&self, grid_x: i32, grid_z: i32) -> Vec3 {
        Vec3::new(
            grid_x as f32 * Self::tile_size() - self.half_extent_x(),
            0.0,
            grid_z as f32 * Self::tile_size() - self.half_extent_z(),
        )
    }

    /// World-space position of a door slot, relative to room center.
    /// Returns the position at the room edge (not beyond it).
    pub fn door_world_offset(&self, door: &DoorSlot) -> Vec3 {
        let ts = Self::tile_size();
        let axis = door.axis_center() * ts;
        match door.side {
            DoorSide::North => Vec3::new(axis - self.half_extent_x(), 0.0, self.half_extent_z()),
            DoorSide::South => Vec3::new(axis - self.half_extent_x(), 0.0, -self.half_extent_z()),
            DoorSide::East => Vec3::new(self.half_extent_x(), 0.0, axis - self.half_extent_z()),
            DoorSide::West => Vec3::new(-self.half_extent_x(), 0.0, axis - self.half_extent_z()),
        }
    }
}

/// Build the built-in template library.
pub fn built_in_templates() -> Vec<RoomTemplate> {
    vec![
        // ---- Start room: small, safe ----
        RoomTemplate {
            width: 12,
            height: 12,
            doors: vec![
                DoorSlot {
                    side: DoorSide::North,
                    position: 6,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::East,
                    position: 6,
                    width: 3,
                },
            ],
            enemies: vec![],
            tag: RoomTag::Start,
            obelisks: false,
        },
        // ---- Small combat room ----
        RoomTemplate {
            width: 14,
            height: 14,
            doors: vec![
                DoorSlot {
                    side: DoorSide::South,
                    position: 7,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::North,
                    position: 7,
                    width: 3,
                },
            ],
            enemies: vec![
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 4,
                    grid_z: 5,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 9,
                    grid_z: 8,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 7,
                    grid_z: 11,
                },
            ],
            tag: RoomTag::Combat,
            obelisks: false,
        },
        // ---- Medium combat room ----
        RoomTemplate {
            width: 18,
            height: 14,
            doors: vec![
                DoorSlot {
                    side: DoorSide::West,
                    position: 7,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::East,
                    position: 7,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::North,
                    position: 9,
                    width: 3,
                },
            ],
            enemies: vec![
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 3,
                    grid_z: 4,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 14,
                    grid_z: 10,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 8,
                    grid_z: 7,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 11,
                    grid_z: 5,
                },
            ],
            tag: RoomTag::Combat,
            obelisks: false,
        },
        // ---- Large arena (original 20x20) ----
        RoomTemplate {
            width: 20,
            height: 20,
            doors: vec![
                DoorSlot {
                    side: DoorSide::South,
                    position: 10,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::North,
                    position: 10,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::East,
                    position: 10,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::West,
                    position: 10,
                    width: 3,
                },
            ],
            enemies: vec![
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 6,
                    grid_z: 8,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 8,
                    grid_z: 14,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 13,
                    grid_z: 6,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 15,
                    grid_z: 11,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 4,
                    grid_z: 5,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 16,
                    grid_z: 15,
                },
            ],
            tag: RoomTag::Combat,
            obelisks: true,
        },
        // ---- Elite room: tough enemies ----
        RoomTemplate {
            width: 16,
            height: 20,
            doors: vec![
                DoorSlot {
                    side: DoorSide::South,
                    position: 8,
                    width: 3,
                },
                DoorSlot {
                    side: DoorSide::North,
                    position: 8,
                    width: 3,
                },
            ],
            enemies: vec![
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 4,
                    grid_z: 5,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 11,
                    grid_z: 5,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 8,
                    grid_z: 10,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 4,
                    grid_z: 14,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 11,
                    grid_z: 14,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 3,
                    grid_z: 10,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 12,
                    grid_z: 10,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 8,
                    grid_z: 17,
                },
            ],
            tag: RoomTag::Elite,
            obelisks: true,
        },
        // ---- Boss room ----
        RoomTemplate {
            width: 22,
            height: 22,
            doors: vec![DoorSlot {
                side: DoorSide::South,
                position: 11,
                width: 4,
            }],
            enemies: vec![
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 5,
                    grid_z: 6,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 16,
                    grid_z: 6,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 5,
                    grid_z: 16,
                },
                EnemySpawn {
                    kind: EnemySlot::Rat,
                    grid_x: 16,
                    grid_z: 16,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 7,
                    grid_z: 11,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 14,
                    grid_z: 11,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 11,
                    grid_z: 7,
                },
                EnemySpawn {
                    kind: EnemySlot::Goblin,
                    grid_x: 11,
                    grid_z: 15,
                },
            ],
            tag: RoomTag::Boss,
            obelisks: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_width_door_gaps_match_requested_width() {
        let door = DoorSlot {
            side: DoorSide::North,
            position: 6,
            width: 3,
        };

        let gap_tiles: Vec<_> = (0..14)
            .filter(|&tile| door.axis_range().contains(&tile))
            .collect();
        assert_eq!(gap_tiles, vec![5, 6, 7]);
    }

    #[test]
    fn even_width_door_gaps_match_requested_width() {
        let door = DoorSlot {
            side: DoorSide::South,
            position: 11,
            width: 4,
        };

        let gap_tiles: Vec<_> = (0..22)
            .filter(|&tile| door.axis_range().contains(&tile))
            .collect();
        assert_eq!(gap_tiles, vec![9, 10, 11, 12]);
    }

    #[test]
    fn even_width_doors_center_on_half_tiles() {
        let template = RoomTemplate {
            width: 22,
            height: 22,
            doors: vec![],
            enemies: vec![],
            tag: RoomTag::Boss,
            obelisks: false,
        };
        let door = DoorSlot {
            side: DoorSide::South,
            position: 11,
            width: 4,
        };

        assert_eq!(template.door_world_offset(&door).x, 0.0);
    }
}
