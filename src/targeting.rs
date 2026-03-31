use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::combat::{HitPoints, StunMeter};
use crate::player::PlayerSet;

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
            );
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
    window: Single<&Window>,
    camera_query: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    targetables: Query<(Entity, &GlobalTransform, &Targetable)>,
    mut state: ResMut<TargetState>,
) {
    // Clear dead targets
    if let Some(targeted) = state.targeted
        && targetables.get(targeted).is_err()
    {
        state.targeted = None;
    }

    state.hovered = None;

    if keyboard.just_pressed(KeyCode::Escape) {
        state.targeted = None;
    }

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let (camera, cam_transform) = *camera_query;

    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
        return;
    };

    let mut best: Option<(Entity, f32)> = None;

    for (entity, global_tf, targetable) in &targetables {
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
    glows: Query<(Entity, &HighlightGlow), Changed<HighlightGlow>>,
    children_query: Query<&Children>,
    material_query: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, glow) in &glows {
        let intensity = glow.amount * 0.8;
        let emissive = LinearRgba::new(intensity, intensity, intensity, 1.0);

        let mut stack = vec![entity];
        while let Some(current) = stack.pop() {
            if let Ok(mat_handle) = material_query.get(current)
                && let Some(mat) = materials.get_mut(&mat_handle.0)
            {
                mat.emissive = emissive;
            }
            if let Ok(children) = children_query.get(current) {
                for &child in children.iter() {
                    stack.push(child);
                }
            }
        }
    }
}

fn update_target_ui(
    state: Res<TargetState>,
    targetables: Query<(&Targetable, &HitPoints, &StunMeter)>,
    mut panel_vis: Single<&mut Visibility, With<TargetPanel>>,
    mut text: Single<&mut Text, With<TargetNameText>>,
    mut hp_fill: Single<&mut Node, (With<TargetHpFill>, Without<TargetStunFill>)>,
    mut stun_fill: Single<&mut Node, (With<TargetStunFill>, Without<TargetHpFill>)>,
) {
    let display_entity = state.hovered.or(state.targeted);

    let show = if let Some(entity) = display_entity {
        if let Ok((targetable, hp, stun)) = targetables.get(entity) {
            text.0 = targetable.name.clone();
            hp_fill.width = Val::Percent(hp.fraction().clamp(0.0, 1.0) * 100.0);
            stun_fill.width = Val::Percent((1.0 - stun.fraction()).clamp(0.0, 1.0) * 100.0);
            true
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
