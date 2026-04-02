use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use smallvec::SmallVec;

use crate::camera::MainCamera;
use crate::combat::{HitPoints, StunMeter};
use crate::enemy::UniqueEnemyMaterialCache;
use crate::hud::PauseMenuState;
use crate::player::{Player, PlayerSet};
use crate::world::tilemap::WallSpatialIndex;

pub struct TargetingPlugin;

impl Plugin for TargetingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TargetState>()
            .add_systems(Startup, spawn_target_ui)
            .add_systems(
                Update,
                (
                    update_hover,
                    update_highlight_glow,
                    apply_highlight_emissive,
                    update_target_ui,
                )
                    .chain()
                    .before(PlayerSet::Update),
            )
            .add_systems(PostUpdate, clear_removed_highlight_emissive);
    }
}

#[derive(Component)]
pub struct Targetable {
    pub name: String,
    pub pick_radius: f32,
}

#[derive(Component)]
pub struct HighlightGlow {
    pub amount: f32,
}

impl Default for HighlightGlow {
    fn default() -> Self {
        Self { amount: 0.0 }
    }
}

#[derive(Resource, Default)]
pub struct TargetState {
    pub hovered: Option<Entity>,
    pub targeted: Option<Entity>,
}

#[derive(Component)]
struct TargetNameText;

#[derive(Component)]
struct TargetHpFill;

#[derive(Component)]
struct TargetStunFill;

#[derive(Component)]
struct TargetPanel;

type TargetableQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Targetable,
        &'static HitPoints,
        &'static StunMeter,
        &'static Visibility,
    ),
    (With<Targetable>, Without<TargetPanel>),
>;

const TARGET_BAR_WIDTH: f32 = 180.0;
const TARGET_BAR_HEIGHT: f32 = 10.0;

fn spawn_target_ui(mut commands: Commands) {
    commands
        .spawn((
            TargetPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            // Name
            parent.spawn((
                TargetNameText,
                Text::new(""),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.78, 0.55)),
            ));

            // HP bar
            parent
                .spawn((
                    Node {
                        width: Val::Px(TARGET_BAR_WIDTH),
                        height: Val::Px(TARGET_BAR_HEIGHT),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.18, 0.04, 0.04)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        TargetHpFill,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.7, 0.12, 0.12)),
                    ));
                });

            // Stun bar
            parent
                .spawn((
                    Node {
                        width: Val::Px(TARGET_BAR_WIDTH),
                        height: Val::Px(6.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.12, 0.11, 0.06)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        TargetStunFill,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.85, 0.80, 0.45)),
                    ));
                });
        });
}

fn update_hover(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    camera_query: Option<Single<(&Camera, &GlobalTransform), With<MainCamera>>>,
    player_tf: Option<Single<&Transform, With<Player>>>,
    pause_menu: Option<Res<PauseMenuState>>,
    wall_index: Res<WallSpatialIndex>,
    targetables: Query<(Entity, &GlobalTransform, &Targetable, &Visibility)>,
    mut state: ResMut<TargetState>,
) {
    let Some(player_tf) = player_tf else {
        state.hovered = None;
        state.targeted = None;
        return;
    };

    // Clear targets that are no longer valid or visible
    if let Some(targeted) = state.targeted {
        let target_visible = targetables.get(targeted).ok().is_some_and(
            |(_, global_tf, _, visibility): (
                Entity,
                &GlobalTransform,
                &Targetable,
                &Visibility,
            )| {
                *visibility != Visibility::Hidden
                    && target_visible_from_player(
                        player_tf.translation,
                        global_tf.translation(),
                        &wall_index,
                    )
            },
        );

        if !target_visible {
            state.targeted = None;
        }
    }

    state.hovered = None;

    if pause_menu
        .as_ref()
        .is_some_and(|pause_menu| pause_menu.open)
    {
        return;
    }

    let Some(window) = window else {
        return;
    };
    let Some(camera_query) = camera_query else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let (camera, cam_transform) = *camera_query;

    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
        return;
    };

    let mut best: Option<(Entity, f32)> = None;

    for (entity, global_tf, targetable, visibility) in &targetables {
        let global_tf: &GlobalTransform = global_tf;
        if *visibility == Visibility::Hidden
            || !target_visible_from_player(
                player_tf.translation,
                global_tf.translation(),
                &wall_index,
            )
        {
            continue;
        }

        let center = global_tf.translation() + Vec3::Y * 0.4;
        let to_center = center - ray.origin;
        let t = to_center.dot(*ray.direction);
        if t < 0.0 {
            continue;
        }

        let closest_on_ray = ray.origin + *ray.direction * t;
        let distance = (center - closest_on_ray).length();

        if distance <= targetable.pick_radius {
            match best {
                Some((_, best_dist)) if distance < best_dist => {
                    best = Some((entity, distance));
                }
                None => {
                    best = Some((entity, distance));
                }
                _ => {}
            }
        }
    }

    state.hovered = best.map(|(e, _)| e);
}

fn target_visible_from_player(
    player_translation: Vec3,
    target_translation: Vec3,
    wall_index: &WallSpatialIndex,
) -> bool {
    wall_index.segment_clear_los(
        Vec2::new(player_translation.x, player_translation.z),
        Vec2::new(target_translation.x, target_translation.z),
        0.0,
    )
}

fn update_highlight_glow(
    state: Res<TargetState>,
    time: Res<Time>,
    mut glows: Query<(Entity, &mut HighlightGlow)>,
) {
    let speed = 10.0;
    let target_amount = 0.4;

    for (entity, mut glow) in &mut glows {
        let highlighted = state.hovered == Some(entity) || state.targeted == Some(entity);
        let goal = if highlighted { target_amount } else { 0.0 };
        let delta = time.delta_secs() * speed;

        if glow.amount < goal {
            glow.amount = (glow.amount + delta).min(goal);
        } else {
            glow.amount = (glow.amount - delta).max(goal);
        }
    }
}

fn apply_highlight_emissive(
    glows: Query<
        (Entity, &HighlightGlow, Option<&UniqueEnemyMaterialCache>),
        Changed<HighlightGlow>,
    >,
    children_query: Query<&Children>,
    material_query: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, glow, material_cache) in &glows {
        let intensity = glow.amount * 0.8;
        let emissive = LinearRgba::new(intensity, intensity, intensity, 1.0);
        if let Some(material_cache) = material_cache {
            set_emissive_cached(&material_cache.handles, emissive, &mut materials);
        } else {
            set_emissive_recursive(
                entity,
                emissive,
                &children_query,
                &material_query,
                &mut materials,
            );
        }
    }
}

fn clear_removed_highlight_emissive(
    mut removed_glows: RemovedComponents<HighlightGlow>,
    material_caches: Query<&UniqueEnemyMaterialCache>,
    children_query: Query<&Children>,
    material_query: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in removed_glows.read() {
        let emissive = LinearRgba::new(0.0, 0.0, 0.0, 1.0);
        if let Ok(material_cache) = material_caches.get(entity) {
            set_emissive_cached(&material_cache.handles, emissive, &mut materials);
        } else {
            set_emissive_recursive(
                entity,
                emissive,
                &children_query,
                &material_query,
                &mut materials,
            );
        }
    }
}

fn set_emissive_cached(
    handles: &[Handle<StandardMaterial>],
    emissive: LinearRgba,
    materials: &mut Assets<StandardMaterial>,
) {
    for handle in handles {
        if let Some(mat) = materials.get_mut(handle) {
            mat.emissive = emissive;
        }
    }
}

fn set_emissive_recursive(
    entity: Entity,
    emissive: LinearRgba,
    children_query: &Query<&Children>,
    material_query: &Query<&MeshMaterial3d<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut stack = SmallVec::<[Entity; 16]>::new();
    stack.push(entity);
    while let Some(current) = stack.pop() {
        if let Ok(mat_handle) = material_query.get(current) {
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.emissive = emissive;
            }
        }
        if let Ok(children) = children_query.get(current) {
            for &child in children.iter() {
                stack.push(child);
            }
        }
    }
}

fn update_target_ui(
    state: Res<TargetState>,
    targetables: TargetableQuery<'_, '_>,
    mut panel_vis: Single<&mut Visibility, With<TargetPanel>>,
    mut text: Single<&mut Text, With<TargetNameText>>,
    mut hp_fill: Single<&mut Node, (With<TargetHpFill>, Without<TargetStunFill>)>,
    mut stun_fill: Single<&mut Node, (With<TargetStunFill>, Without<TargetHpFill>)>,
) {
    let display_entity = state.hovered.or(state.targeted);

    let show = if let Some(entity) = display_entity {
        if let Ok((targetable, hp, stun, visibility)) = targetables.get(entity) {
            if *visibility == Visibility::Hidden {
                false
            } else {
                text.0 = targetable.name.clone();
                hp_fill.width = Val::Percent(hp.fraction().clamp(0.0, 1.0) * 100.0);
                stun_fill.width = Val::Percent((1.0 - stun.fraction()).clamp(0.0, 1.0) * 100.0);
                true
            }
        } else {
            false
        }
    } else {
        false
    };

    **panel_vis = if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tilemap::{WallSegment, WallSpatialIndex};

    #[test]
    fn target_visibility_rejects_blocked_targets() {
        let mut wall_index = WallSpatialIndex::default();
        wall_index.rebuild(vec![WallSegment {
            center: Vec2::new(5.0, 0.0),
            half_extents: Vec2::new(1.0, 1.0),
            blocks_los: true,
        }]);

        assert!(!target_visible_from_player(
            Vec3::ZERO,
            Vec3::new(10.0, 0.0, 0.0),
            &wall_index,
        ));
    }

    #[test]
    fn target_visibility_allows_clear_targets() {
        let mut wall_index = WallSpatialIndex::default();
        wall_index.rebuild(vec![WallSegment {
            center: Vec2::new(5.0, 3.0),
            half_extents: Vec2::new(1.0, 1.0),
            blocks_los: true,
        }]);

        assert!(target_visible_from_player(
            Vec3::ZERO,
            Vec3::new(10.0, 0.0, 0.0),
            &wall_index,
        ));
    }
}
