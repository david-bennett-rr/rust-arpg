use bevy::prelude::*;

use crate::combat::{GameOver, HitFlash, HitPoints};
use crate::enemy::RespawnEnemies;
use crate::player::{
    DeathAnim, Dodge, KnightAnimator, MoveTarget, Player, PlayerCombat, PlayerStats,
};
use crate::targeting::TargetState;
use crate::world::tilemap::grid_to_world;

const BAR_WIDTH: f32 = 220.0;
const BAR_HEIGHT: f32 = 14.0;
const BAR_GAP: f32 = 4.0;
const BAR_LEFT: f32 = 16.0;
const BAR_TOP: f32 = 16.0;

const HP_COLOR: Color = Color::srgb(0.7, 0.12, 0.12);
const HP_BG: Color = Color::srgb(0.18, 0.04, 0.04);
const STAMINA_COLOR: Color = Color::srgb(0.22, 0.6, 0.18);
const STAMINA_BG: Color = Color::srgb(0.06, 0.14, 0.05);
const MANA_COLOR: Color = Color::srgb(0.15, 0.25, 0.7);
const MANA_BG: Color = Color::srgb(0.04, 0.06, 0.18);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hud, spawn_death_screen))
            .add_systems(Update, (update_hud, check_death, handle_restart_click));
    }
}

// ---------------------------------------------------------------------------
// Stat bars
// ---------------------------------------------------------------------------

#[derive(Component)]
struct HudBarFill(BarKind);

#[derive(Clone, Copy)]
enum BarKind {
    Health,
    Stamina,
    Mana,
}

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(BAR_LEFT),
            top: Val::Px(BAR_TOP),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(BAR_GAP),
            ..default()
        })
        .with_children(|parent| {
            spawn_bar(parent, BarKind::Health, HP_COLOR, HP_BG);
            spawn_bar(parent, BarKind::Stamina, STAMINA_COLOR, STAMINA_BG);
            spawn_bar(parent, BarKind::Mana, MANA_COLOR, MANA_BG);
        });
}

fn spawn_bar(parent: &mut ChildBuilder, kind: BarKind, fill_color: Color, bg_color: Color) {
    parent
        .spawn(Node {
            width: Val::Px(BAR_WIDTH),
            height: Val::Px(BAR_HEIGHT),
            ..default()
        })
        .insert(BackgroundColor(bg_color))
        .with_children(|bar| {
            bar.spawn((
                HudBarFill(kind),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(fill_color),
            ));
        });
}

fn update_hud(
    player: Option<Single<(&HitPoints, &PlayerStats), With<Player>>>,
    mut fills: Query<(&HudBarFill, &mut Node)>,
) {
    let Some(player) = player else {
        return;
    };
    let (hp, stats) = *player;

    for (bar, mut node) in &mut fills {
        let pct = match bar.0 {
            BarKind::Health => hp.fraction(),
            BarKind::Stamina => stats.stamina / stats.max_stamina,
            BarKind::Mana => stats.mana / stats.max_mana,
        };
        node.width = Val::Percent(pct.clamp(0.0, 1.0) * 100.0);
    }
}

// ---------------------------------------------------------------------------
// Death screen
// ---------------------------------------------------------------------------

#[derive(Component)]
struct DeathScreen;

#[derive(Component)]
struct RestartButton;

type RestartButtonInteractions<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static mut BackgroundColor),
    (Changed<Interaction>, With<RestartButton>),
>;

type RestartPlayer<'w> = Option<
    Single<
        'w,
        (
            &'static mut Transform,
            &'static mut HitPoints,
            &'static mut PlayerStats,
            &'static mut PlayerCombat,
            &'static mut Dodge,
            &'static mut MoveTarget,
            &'static mut DeathAnim,
            &'static mut KnightAnimator,
            &'static mut HitFlash,
        ),
        With<Player>,
    >,
>;

fn spawn_death_screen(mut commands: Commands) {
    commands
        .spawn((
            DeathScreen,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(24.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
            Visibility::Hidden,
            // High z-index so it renders on top
            GlobalZIndex(10),
        ))
        .with_children(|parent| {
            // YOU DIED text
            parent.spawn((
                Text::new("YOU DIED"),
                TextFont {
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.10, 0.08)),
            ));

            // RESTART button
            parent
                .spawn((
                    RestartButton,
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(28.0), Val::Px(12.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("RESTART"),
                        TextFont {
                            font_size: 26.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.85, 0.85, 0.85)),
                    ));
                });
        });
}

fn check_death(
    mut game_over: ResMut<GameOver>,
    player: Option<Single<(&HitPoints, &DeathAnim), With<Player>>>,
    mut death_screen: Query<&mut Visibility, With<DeathScreen>>,
) {
    let Some(player) = player else {
        return;
    };
    let (hp, death_anim) = *player;

    // Freeze gameplay immediately on death
    if hp.is_dead() && !game_over.0 {
        game_over.0 = true;
    }

    // Show death screen only after the fall animation finishes
    if game_over.0 && death_anim.active && death_anim.timer.finished() {
        for mut vis in &mut death_screen {
            *vis = Visibility::Visible;
        }
    }
}

fn handle_restart_click(
    mut game_over: ResMut<GameOver>,
    mut interaction_query: RestartButtonInteractions<'_, '_>,
    mut death_screen: Query<&mut Visibility, With<DeathScreen>>,
    mut player: RestartPlayer<'_>,
    mut target_state: ResMut<TargetState>,
    mut respawn_event: EventWriter<RespawnEnemies>,
) {
    for (interaction, mut bg) in &mut interaction_query {
        match interaction {
            Interaction::Hovered => {
                *bg = BackgroundColor(Color::srgb(0.35, 0.35, 0.35));
            }
            Interaction::Pressed => {
                *bg = BackgroundColor(Color::srgb(0.45, 0.45, 0.45));

                // Reset game
                game_over.0 = false;
                for mut vis in &mut death_screen {
                    *vis = Visibility::Hidden;
                }

                // Reset player
                if let Some(ref mut p) = player {
                    let (
                        ref mut tf,
                        ref mut hp,
                        ref mut stats,
                        ref mut combat,
                        ref mut dodge,
                        ref mut move_target,
                        ref mut death_anim,
                        ref mut animator,
                        ref mut flash,
                    ) = **p;
                    tf.translation = grid_to_world(10, 10);
                    tf.rotation = Quat::IDENTITY;
                    hp.current = hp.max;
                    **stats = PlayerStats::default();
                    **combat = PlayerCombat::default();
                    **dodge = Dodge::default();
                    **death_anim = DeathAnim::default();
                    **animator = KnightAnimator::default();
                    **flash = HitFlash::default();
                    move_target.position = None;
                }

                // Clear target
                target_state.hovered = None;
                target_state.targeted = None;

                // Respawn all enemies
                respawn_event.send(RespawnEnemies);
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgb(0.25, 0.25, 0.25));
            }
        }
    }
}
