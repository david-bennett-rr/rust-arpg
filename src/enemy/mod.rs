mod arrow;
mod goblin;
mod rat;

use std::collections::HashMap;

use bevy::{asset::AssetId, prelude::*};
use smallvec::SmallVec;

use crate::combat::{DamageRng, HitFlash, HitPoints, StunMeter, game_running};
use crate::player::{
    Dodge, PLAYER_COLLISION_RADIUS, Player, PlayerCombat, PlayerSet, visual_forward,
};
use crate::targeting::TargetState;
use crate::world::tilemap::clamp_translation_to_arena;

pub use arrow::Arrow;
pub use goblin::GoblinArcher;
pub use rat::DemonRat;

#[derive(Event)]
pub struct RespawnEnemies;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RespawnEnemies>()
            .add_systems(PreUpdate, instance_enemy_materials)
            .add_systems(
                Startup,
                (rat::spawn_demon_rats, arrow::setup_arrow_meshes, goblin::spawn_goblin_archers).chain(),
            )
            .add_systems(
                Update,
                (
                    rat::update_demon_rats,
                    goblin::update_goblin_archers,
                    arrow::update_arrows,
                    rat::animate_demon_rats,
                    goblin::animate_goblin_archers,
                    resolve_actor_collisions,
                )
                    .chain()
                    .run_if(game_running)
                    .after(PlayerSet::Update),
            )
            .add_systems(Update, handle_respawn_enemies);
    }
}

/// Per-enemy collision settings.
#[derive(Component, Clone, Copy)]
pub(crate) struct EnemyCollision {
    pub(crate) radius: f32,
    /// Share of overlap correction applied to the player (0.0 = enemy absorbs
    /// all push, 1.0 = player absorbs all push).
    pub(crate) player_push_share: f32,
}

#[derive(Component)]
pub(crate) struct UniqueEnemyMaterials;

type CollisionPlayer<'w> =
    Single<'w, (&'static mut Transform, &'static Dodge), (With<Player>, Without<EnemyCollision>)>;

type EnemyActors<'w, 's> = Query<
    'w,
    's,
    (&'static EnemyCollision, &'static mut Transform),
    (With<EnemyCollision>, Without<Player>),
>;

#[derive(Clone, Copy)]
pub(crate) struct PlayerSlashState<'a> {
    pub(crate) ground: Vec3,
    pub(crate) forward: Vec3,
    pub(crate) combat: &'a PlayerCombat,
    pub(crate) ready: bool,
}

pub(crate) struct SlashTarget<'a> {
    pub(crate) last_hit_swing_id: &'a mut u32,
    pub(crate) health: &'a mut HitPoints,
    pub(crate) stun: &'a mut StunMeter,
    pub(crate) flash: &'a mut HitFlash,
}

const PLAYER_SLASH_RANGE: f32 = 2.5;
const PLAYER_SLASH_ARC_DOT: f32 = 0.08;
const COLLISION_EPSILON: f32 = 0.001;

pub(crate) fn build_player_slash_state<'a>(
    transform: &Transform,
    combat: &'a PlayerCombat,
) -> PlayerSlashState<'a> {
    let ground = Vec3::new(transform.translation.x, 0.0, transform.translation.z);
    let forward = visual_forward(transform).normalize_or_zero();
    PlayerSlashState {
        ground,
        forward,
        combat,
        ready: combat.strike > 0.42,
    }
}

/// Returns `true` if the entity was killed and despawned.
pub(crate) fn try_player_slash(
    commands: &mut Commands,
    entity: Entity,
    enemy_ground: Vec3,
    target: SlashTarget<'_>,
    player_slash: PlayerSlashState<'_>,
    damage_rng: &mut DamageRng,
    target_state: &mut TargetState,
) -> bool {
    if !player_slash.ready || player_slash.combat.swing_id == *target.last_hit_swing_id {
        return false;
    }
    let to_enemy = enemy_ground - player_slash.ground;
    let distance = to_enemy.length();
    let direction = to_enemy.normalize_or_zero();
    if distance <= PLAYER_SLASH_RANGE && direction.dot(player_slash.forward) >= PLAYER_SLASH_ARC_DOT
    {
        *target.last_hit_swing_id = player_slash.combat.swing_id;
        let damage = damage_rng.roll_1d5();
        if target.health.apply_damage(damage) > 0 {
            target.stun.apply_stun_damage(damage as f32);
            target.flash.trigger();
            if target.health.is_dead() {
                // Clear targeting refs before despawn to avoid stale entity access
                if target_state.targeted == Some(entity) {
                    target_state.targeted = None;
                }
                if target_state.hovered == Some(entity) {
                    target_state.hovered = None;
                }
                commands.entity(entity).despawn_recursive();
                return true;
            }
        }
    }
    false
}

fn resolve_actor_collisions(
    mut player: CollisionPlayer<'_>,
    enemy_entities: Query<Entity, With<EnemyCollision>>,
    mut enemy_transforms: EnemyActors<'_, '_>,
) {
    let (ref mut player_tf, dodge) = *player;
    let mut player_ground = horizontal_position(player_tf.translation);
    let enemy_ids: SmallVec<[Entity; 16]> = enemy_entities.iter().collect();

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
                collision.player_push_share,
            );

            if separation {
                player_tf.translation.x = player_ground.x;
                player_tf.translation.z = player_ground.y;
                enemy_transform.translation.x = enemy_ground.x;
                enemy_transform.translation.z = enemy_ground.y;
                clamp_translation_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
                clamp_translation_to_arena(&mut enemy_transform.translation, collision.radius);
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
                clamp_translation_to_arena(&mut enemy_a.translation, collision_a.radius);
                clamp_translation_to_arena(&mut enemy_b.translation, collision_b.radius);
            }
        }
    }
}

fn instance_enemy_materials(
    roots: Query<Entity, Added<UniqueEnemyMaterials>>,
    children_query: Query<&Children>,
    mut material_query: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for root in &roots {
        let mut stack = vec![root];
        let mut cloned_handles: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>> =
            HashMap::new();

        while let Some(entity) = stack.pop() {
            if let Ok(mut material_handle) = material_query.get_mut(entity) {
                let source_id = material_handle.0.id();
                let cloned_handle = if let Some(existing) = cloned_handles.get(&source_id) {
                    existing.clone()
                } else {
                    let Some(material) = materials.get(&material_handle.0).cloned() else {
                        continue;
                    };
                    let cloned = materials.add(material);
                    cloned_handles.insert(source_id, cloned.clone());
                    cloned
                };

                material_handle.0 = cloned_handle;
            }

            if let Ok(children) = children_query.get(entity) {
                stack.extend(children.iter().copied());
            }
        }
    }
}

pub(crate) fn horizontal_position(translation: Vec3) -> Vec2 {
    Vec2::new(translation.x, translation.z)
}

pub(crate) fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    horizontal_position(a - b).length()
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

fn handle_respawn_enemies(
    mut commands: Commands,
    mut events: EventReader<RespawnEnemies>,
    rats: Query<Entity, With<DemonRat>>,
    goblins: Query<Entity, With<GoblinArcher>>,
    arrows: Query<Entity, With<Arrow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if events.is_empty() {
        return;
    }
    events.clear();

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
    rat::do_spawn_rats(&mut commands, &mut meshes, &mut materials);
    goblin::do_spawn_goblins(&mut commands, &mut meshes, &mut materials);
}
