use bevy::{input::gamepad::Gamepad, prelude::*};

use crate::combat::{GameOver, HitFlash, HitPoints};
use crate::enemy::RespawnEnemies;
use crate::player::{
    ControllerMove, DeathAnim, Dodge, KnightAnimator, MoveTarget, Player, PlayerCombat,
    PlayerStats,
};
use crate::targeting::TargetState;
use crate::world::tilemap::{PLAYER_SPAWN_GRID, grid_to_world};

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
        app.init_resource::<PauseMenuState>()
            .add_systems(Startup, (spawn_hud, spawn_pause_menu, spawn_death_screen))
            .add_systems(
                PreUpdate,
                (
                    toggle_pause_menu,
                    navigate_pause_menu,
                    activate_pause_menu_selection,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    update_hud,
                    sync_pause_menu_visibility,
                    sync_pause_menu_buttons,
                    handle_pause_menu_click,
                    check_death,
                    handle_restart_click,
                ),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PauseMenuAction {
    Resume,
    RestartLevel,
}

#[derive(Resource)]
pub struct PauseMenuState {
    pub open: bool,
    selected: PauseMenuAction,
}

impl Default for PauseMenuState {
    fn default() -> Self {
        Self {
            open: false,
            selected: PauseMenuAction::Resume,
        }
    }
}

impl PauseMenuState {
    fn open(&mut self) {
        self.open = true;
        self.selected = PauseMenuAction::Resume;
    }

    fn close(&mut self) {
        self.open = false;
        self.selected = PauseMenuAction::Resume;
    }

    fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    fn select_previous(&mut self) {
        self.selected = match self.selected {
            PauseMenuAction::Resume => PauseMenuAction::RestartLevel,
            PauseMenuAction::RestartLevel => PauseMenuAction::Resume,
        };
    }

    fn select_next(&mut self) {
        self.select_previous();
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

#[derive(Component)]
struct PauseMenu;

#[derive(Component, Clone, Copy)]
struct PauseMenuButton(PauseMenuAction);

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
            &'static mut ControllerMove,
            &'static mut DeathAnim,
            &'static mut KnightAnimator,
            &'static mut HitFlash,
        ),
        With<Player>,
    >,
>;

fn spawn_pause_menu(mut commands: Commands) {
    commands
        .spawn((
            PauseMenu,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.58)),
            Visibility::Hidden,
            GlobalZIndex(9),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(320.0),
                        padding: UiRect::all(Val::Px(24.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Stretch,
                        row_gap: Val::Px(14.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.09, 0.94)),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("PAUSED"),
                        TextFont {
                            font_size: 36.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.92, 0.88, 0.76)),
                    ));

                    panel.spawn((
                        Text::new("Esc / Start to resume"),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.72, 0.72, 0.72)),
                    ));

                    spawn_pause_button(panel, PauseMenuAction::Resume, "RESUME");
                    spawn_pause_button(panel, PauseMenuAction::RestartLevel, "RESTART LEVEL");
                });
        });
}

fn spawn_pause_button(parent: &mut ChildBuilder, action: PauseMenuAction, label: &str) {
    parent
        .spawn((
            PauseMenuButton(action),
            Button,
            Node {
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(20.0), Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.19, 0.19, 0.19)),
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
            ));
        });
}

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
    mut pause_menu: ResMut<PauseMenuState>,
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
        pause_menu.close();
    }

    // Show death screen only after the fall animation finishes
    if game_over.0 && death_anim.active && death_anim.timer.finished() {
        for mut vis in &mut death_screen {
            *vis = Visibility::Visible;
        }
    }
}

fn toggle_pause_menu(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    game_over: Res<GameOver>,
    mut pause_menu: ResMut<PauseMenuState>,
) {
    if game_over.0 {
        return;
    }

    let start_pressed = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::Start));
    if keyboard.just_pressed(KeyCode::Escape) || start_pressed {
        pause_menu.toggle();
    }
}

fn navigate_pause_menu(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut stick_released: Local<bool>,
) {
    if !pause_menu.open {
        *stick_released = true;
        return;
    }

    let navigate_up = keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW);
    let navigate_down =
        keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS);

    let stick_y = gamepads.iter().fold(0.0_f32, |best, gamepad| {
        let candidate = gamepad.dpad().y;
        if candidate.abs() >= best.abs() {
            candidate
        } else {
            let stick = gamepad.left_stick().y;
            if stick.abs() >= best.abs() {
                stick
            } else {
                best
            }
        }
    });
    let stick_active = stick_y.abs() >= 0.5;
    let stick_up = stick_active && *stick_released && stick_y > 0.0;
    let stick_down = stick_active && *stick_released && stick_y < 0.0;

    if navigate_up || stick_up {
        pause_menu.select_previous();
    } else if navigate_down || stick_down {
        pause_menu.select_next();
    }

    *stick_released = !stick_active;
}

fn activate_pause_menu_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut game_over: ResMut<GameOver>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut death_screen: Query<&mut Visibility, With<DeathScreen>>,
    mut pause_menu_visibility: Query<&mut Visibility, (With<PauseMenu>, Without<DeathScreen>)>,
    mut player: RestartPlayer<'_>,
    mut target_state: ResMut<TargetState>,
    mut respawn_event: EventWriter<RespawnEnemies>,
) {
    if !pause_menu.open {
        return;
    }

    let confirm = keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::Space)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::South));
    if !confirm {
        return;
    }

    execute_pause_menu_action(
        pause_menu.selected,
        &mut game_over,
        &mut pause_menu,
        &mut death_screen,
        &mut pause_menu_visibility,
        &mut player,
        &mut target_state,
        &mut respawn_event,
    );
}

fn sync_pause_menu_visibility(
    pause_menu: Res<PauseMenuState>,
    mut pause_menu_query: Query<&mut Visibility, With<PauseMenu>>,
) {
    let visibility = if pause_menu.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut node_visibility in &mut pause_menu_query {
        *node_visibility = visibility;
    }
}

fn sync_pause_menu_buttons(
    pause_menu: Res<PauseMenuState>,
    mut buttons: Query<(&PauseMenuButton, &Interaction, &mut BackgroundColor)>,
) {
    for (button, interaction, mut background) in &mut buttons {
        let color = if pause_menu.open && pause_menu.selected == button.0 {
            Color::srgb(0.52, 0.38, 0.18)
        } else if pause_menu.open && *interaction == Interaction::Hovered {
            Color::srgb(0.32, 0.32, 0.32)
        } else {
            Color::srgb(0.19, 0.19, 0.19)
        };
        *background = BackgroundColor(color);
    }
}

fn handle_pause_menu_click(
    mut game_over: ResMut<GameOver>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut buttons: Query<(&Interaction, &PauseMenuButton), (Changed<Interaction>, With<Button>)>,
    mut death_screen: Query<&mut Visibility, With<DeathScreen>>,
    mut pause_menu_visibility: Query<&mut Visibility, (With<PauseMenu>, Without<DeathScreen>)>,
    mut player: RestartPlayer<'_>,
    mut target_state: ResMut<TargetState>,
    mut respawn_event: EventWriter<RespawnEnemies>,
) {
    if !pause_menu.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        match interaction {
            Interaction::Hovered => {
                pause_menu.selected = button.0;
            }
            Interaction::Pressed => {
                pause_menu.selected = button.0;
                execute_pause_menu_action(
                    button.0,
                    &mut game_over,
                    &mut pause_menu,
                    &mut death_screen,
                    &mut pause_menu_visibility,
                    &mut player,
                    &mut target_state,
                    &mut respawn_event,
                );
            }
            Interaction::None => {}
        }
    }
}

fn handle_restart_click(
    mut game_over: ResMut<GameOver>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut interaction_query: RestartButtonInteractions<'_, '_>,
    mut death_screen: Query<&mut Visibility, With<DeathScreen>>,
    mut pause_menu_visibility: Query<&mut Visibility, (With<PauseMenu>, Without<DeathScreen>)>,
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
                restart_level(
                    &mut game_over,
                    &mut pause_menu,
                    &mut death_screen,
                    &mut pause_menu_visibility,
                    &mut player,
                    &mut target_state,
                    &mut respawn_event,
                );
            }
            Interaction::None => {
                *bg = BackgroundColor(Color::srgb(0.25, 0.25, 0.25));
            }
        }
    }
}

fn execute_pause_menu_action(
    action: PauseMenuAction,
    game_over: &mut GameOver,
    pause_menu: &mut PauseMenuState,
    death_screen: &mut Query<&mut Visibility, With<DeathScreen>>,
    pause_menu_visibility: &mut Query<&mut Visibility, (With<PauseMenu>, Without<DeathScreen>)>,
    player: &mut RestartPlayer<'_>,
    target_state: &mut ResMut<TargetState>,
    respawn_event: &mut EventWriter<RespawnEnemies>,
) {
    match action {
        PauseMenuAction::Resume => {
            pause_menu.close();
            for mut vis in pause_menu_visibility.iter_mut() {
                *vis = Visibility::Hidden;
            }
        }
        PauseMenuAction::RestartLevel => {
            restart_level(
                game_over,
                pause_menu,
                death_screen,
                pause_menu_visibility,
                player,
                target_state,
                respawn_event,
            );
        }
    }
}

fn restart_level(
    game_over: &mut GameOver,
    pause_menu: &mut PauseMenuState,
    death_screen: &mut Query<&mut Visibility, With<DeathScreen>>,
    pause_menu_visibility: &mut Query<&mut Visibility, (With<PauseMenu>, Without<DeathScreen>)>,
    player: &mut RestartPlayer<'_>,
    target_state: &mut ResMut<TargetState>,
    respawn_event: &mut EventWriter<RespawnEnemies>,
) {
    game_over.0 = false;
    pause_menu.close();

    for mut vis in death_screen.iter_mut() {
        *vis = Visibility::Hidden;
    }
    for mut vis in pause_menu_visibility.iter_mut() {
        *vis = Visibility::Hidden;
    }

    if let Some(p) = player {
        let (
            ref mut tf,
            ref mut hp,
            ref mut stats,
            ref mut combat,
            ref mut dodge,
            ref mut move_target,
            ref mut controller_move,
            ref mut death_anim,
            ref mut animator,
            ref mut flash,
        ) = **p;
        tf.translation = grid_to_world(PLAYER_SPAWN_GRID.0, PLAYER_SPAWN_GRID.1);
        tf.rotation = Quat::IDENTITY;
        hp.current = hp.max;
        **stats = PlayerStats::default();
        **combat = PlayerCombat::default();
        **dodge = Dodge::default();
        **controller_move = ControllerMove::default();
        **death_anim = DeathAnim::default();
        **animator = KnightAnimator::default();
        **flash = HitFlash::default();
        move_target.position = None;
    }

    target_state.hovered = None;
    target_state.targeted = None;
    respawn_event.send(RespawnEnemies);
}
