use bevy::ecs::system::SystemParam;
use bevy::{input::gamepad::Gamepad, prelude::*};

use crate::combat::{GameOver, HitFlash, HitPoints};
use crate::enemy::RespawnEnemies;
use crate::player::{
    AttackLunge, ControllerMove, DeathAnim, Dodge, HealingFlask, KnightAnimator, MoveTarget,
    Player, PlayerCombat, PlayerSet, PlayerStats,
};
use crate::targeting::TargetState;
use crate::world::Bonfire;

const BAR_WIDTH: f32 = 220.0;
const BAR_HEIGHT: f32 = 14.0;
const BAR_GAP: f32 = 4.0;
const BAR_LEFT: f32 = 16.0;
const BAR_TOP: f32 = 16.0;

const HP_COLOR: Color = Color::srgb(0.7, 0.12, 0.12);
const HP_BG: Color = Color::srgb(0.18, 0.04, 0.04);
const STAMINA_COLOR: Color = Color::srgb(0.22, 0.6, 0.18);
const STAMINA_BG: Color = Color::srgb(0.06, 0.14, 0.05);
const BAR_FLASH_DURATION: f32 = 0.25;

const DEATH_TEXT_DELAY: f32 = 0.5;
const DEATH_FADE_OUT_DURATION: f32 = 2.0;
const DEATH_FADE_IN_DURATION: f32 = 1.5;

#[derive(Event)]
pub struct StaminaFlashEvent;

#[derive(Resource)]
pub struct LastBonfirePosition(pub Vec3);

impl Default for LastBonfirePosition {
    fn default() -> Self {
        Self(Vec3::ZERO)
    }
}

#[derive(Default)]
enum DeathPhase {
    #[default]
    Alive,
    /// Death anim playing, waiting for it to finish
    Dying,
    /// "YOU DIED" text visible, fading to black
    FadeOut(f32),
    /// Fully black, respawning
    Respawn,
    /// Fading back in at bonfire
    FadeIn(f32),
}

#[derive(Resource, Default)]
struct DeathRespawnState {
    phase: DeathPhase,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenuState>()
            .init_resource::<LastBonfirePosition>()
            .init_resource::<DeathRespawnState>()
            .add_event::<StaminaFlashEvent>()
            .add_systems(Startup, (spawn_hud, spawn_pause_menu, spawn_death_screen, init_bonfire_position))
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
                    flash_hud_bars,
                    sync_pause_menu_visibility,
                    death_respawn_sequence,
                    (
                        handle_pause_menu_click,
                        sync_pause_menu_buttons,
                    )
                        .chain()
                        .before(PlayerSet::Update),
                ),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PauseMenuAction {
    Resume,
    RestartLevel,
    Quit,
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
            PauseMenuAction::Resume => PauseMenuAction::Quit,
            PauseMenuAction::RestartLevel => PauseMenuAction::Resume,
            PauseMenuAction::Quit => PauseMenuAction::RestartLevel,
        };
    }

    fn select_next(&mut self) {
        self.selected = match self.selected {
            PauseMenuAction::Resume => PauseMenuAction::RestartLevel,
            PauseMenuAction::RestartLevel => PauseMenuAction::Quit,
            PauseMenuAction::Quit => PauseMenuAction::Resume,
        };
    }
}

// ---------------------------------------------------------------------------
// Stat bars
// ---------------------------------------------------------------------------

#[derive(Component)]
struct HudBarFill(BarKind);

#[derive(Component)]
struct BarFlash {
    timer: Timer,
    base_color: Color,
}

impl BarFlash {
    fn new(base_color: Color) -> Self {
        let mut timer = Timer::from_seconds(BAR_FLASH_DURATION, TimerMode::Once);
        timer.tick(std::time::Duration::from_secs_f32(BAR_FLASH_DURATION));
        Self { timer, base_color }
    }

    fn trigger(&mut self) {
        self.timer.reset();
    }

    fn amount(&self) -> f32 {
        if self.timer.finished() {
            return 0.0;
        }
        let duration = self.timer.duration().as_secs_f32().max(0.001);
        (1.0 - self.timer.elapsed_secs() / duration).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy)]
enum BarKind {
    Health,
    Stamina,
}

const FLASK_COLOR: Color = Color::srgb(0.85, 0.55, 0.12);

#[derive(Component)]
struct FlaskChargeText;

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(BAR_LEFT),
            top: Val::Px(BAR_TOP),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|parent| {
            // Flask icon + charge count
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|flask_col| {
                    // Bottle shape (a colored rectangle as a simple icon)
                    flask_col.spawn((
                        Node {
                            width: Val::Px(18.0),
                            height: Val::Px(24.0),
                            ..default()
                        },
                        BackgroundColor(FLASK_COLOR),
                    ));
                    // Charge count
                    flask_col.spawn((
                        FlaskChargeText,
                        Text::new("3"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(FLASK_COLOR),
                    ));
                });

            // Bars
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(BAR_GAP),
                    ..default()
                })
                .with_children(|bar_col| {
                    spawn_bar(bar_col, BarKind::Health, HP_COLOR, HP_BG);
                    spawn_bar(bar_col, BarKind::Stamina, STAMINA_COLOR, STAMINA_BG);
                });
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
                BarFlash::new(fill_color),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(fill_color),
            ));
        });
}

#[allow(clippy::type_complexity)]
fn update_hud(
    player: Option<Single<(&HitPoints, &PlayerStats, &HealingFlask), With<Player>>>,
    mut fills: Query<(&HudBarFill, &mut Node)>,
    mut flask_text: Single<&mut Text, With<FlaskChargeText>>,
) {
    let Some(player) = player else {
        return;
    };
    let (hp, stats, flask) = *player;

    for (bar, mut node) in &mut fills {
        let pct = match bar.0 {
            BarKind::Health => hp.fraction(),
            BarKind::Stamina => stats.stamina / stats.max_stamina,
        };
        node.width = Val::Percent(pct.clamp(0.0, 1.0) * 100.0);
    }

    flask_text.0 = format!("{}", flask.charges);
}

fn flash_hud_bars(
    time: Res<Time>,
    player: Option<Single<&HitFlash, With<Player>>>,
    mut stamina_events: EventReader<StaminaFlashEvent>,
    mut fills: Query<(&HudBarFill, &mut BarFlash, &mut BackgroundColor)>,
) {
    let hp_hit = player.is_some_and(|p| p.amount() > 0.5);
    let stamina_fail = !stamina_events.is_empty();
    stamina_events.clear();

    for (bar, mut flash, mut bg) in &mut fills {
        match bar.0 {
            BarKind::Health if hp_hit => flash.trigger(),
            BarKind::Stamina if stamina_fail => flash.trigger(),
            _ => {}
        }

        flash.timer.tick(time.delta());

        let amount = flash.amount();
        if amount > 0.0 {
            let flash_color = Color::WHITE;
            let base = flash.base_color.to_srgba();
            let white = flash_color.to_srgba();
            let blend = amount * 0.6;
            *bg = BackgroundColor(Color::srgb(
                base.red + (white.red - base.red) * blend,
                base.green + (white.green - base.green) * blend,
                base.blue + (white.blue - base.blue) * blend,
            ));
        } else {
            *bg = BackgroundColor(flash.base_color);
        }
    }
}

// ---------------------------------------------------------------------------
// Death screen
// ---------------------------------------------------------------------------

#[derive(Component)]
struct DeathScreen;

#[derive(Component)]
struct DeathText;

#[derive(Component)]
struct PauseMenu;

#[derive(Component, Clone, Copy)]
struct PauseMenuButton(PauseMenuAction);

type PauseMenuButtonInteractions<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static PauseMenuButton),
    (Changed<Interaction>, With<Button>),
>;

type DeathScreenVisibilityQuery<'w, 's> = Query<'w, 's, &'static mut Visibility, With<DeathScreen>>;

type PauseMenuVisibilityQuery<'w, 's> =
    Query<'w, 's, &'static mut Visibility, (With<PauseMenu>, Without<DeathScreen>)>;

#[allow(clippy::type_complexity)]
type RestartPlayer<'w> = Option<
    Single<
        'w,
        (
            &'static mut Transform,
            &'static mut HitPoints,
            &'static mut PlayerStats,
            &'static mut PlayerCombat,
            &'static mut AttackLunge,
            &'static mut Dodge,
            &'static mut HealingFlask,
            &'static mut MoveTarget,
            &'static mut ControllerMove,
            &'static mut DeathAnim,
            &'static mut KnightAnimator,
            &'static mut HitFlash,
        ),
        With<Player>,
    >,
>;

#[derive(SystemParam)]
struct RestartContext<'w, 's> {
    game_over: ResMut<'w, GameOver>,
    pause_menu: ResMut<'w, PauseMenuState>,
    death_screen: DeathScreenVisibilityQuery<'w, 's>,
    pause_menu_visibility: PauseMenuVisibilityQuery<'w, 's>,
    player: RestartPlayer<'w>,
    target_state: ResMut<'w, TargetState>,
    respawn_event: EventWriter<'w, RespawnEnemies>,
    exit_event: EventWriter<'w, AppExit>,
    last_bonfire: Res<'w, LastBonfirePosition>,
}

impl RestartContext<'_, '_> {
    fn execute_pause_menu_action(&mut self, action: PauseMenuAction) {
        match action {
            PauseMenuAction::Resume => self.resume(),
            PauseMenuAction::RestartLevel => self.restart_level(),
            PauseMenuAction::Quit => {
                self.exit_event.send(AppExit::Success);
            }
        }
    }

    fn resume(&mut self) {
        self.pause_menu.close();
        for mut visibility in &mut self.pause_menu_visibility {
            *visibility = Visibility::Hidden;
        }
    }

    fn restart_level(&mut self) {
        for mut visibility in &mut self.death_screen {
            *visibility = Visibility::Hidden;
        }
        self.respawn_at_bonfire(self.last_bonfire.0);
    }

    fn respawn_at_bonfire(&mut self, bonfire_pos: Vec3) {
        self.game_over.0 = false;
        self.pause_menu.close();

        for mut visibility in &mut self.pause_menu_visibility {
            *visibility = Visibility::Hidden;
        }

        if let Some(player) = self.player.as_mut() {
            let (
                ref mut transform,
                ref mut hit_points,
                ref mut stats,
                ref mut combat,
                ref mut attack_lunge,
                ref mut dodge,
                ref mut flask,
                ref mut move_target,
                ref mut controller_move,
                ref mut death_anim,
                ref mut animator,
                ref mut flash,
            ) = **player;
            transform.translation = bonfire_pos + Vec3::new(0.0, 0.0, -2.5);
            transform.rotation = Quat::IDENTITY;
            hit_points.heal_to_full();
            **stats = PlayerStats::default();
            **combat = PlayerCombat::default();
            **attack_lunge = AttackLunge::default();
            **dodge = Dodge::default();
            **flask = HealingFlask::default();
            **controller_move = ControllerMove::default();
            **death_anim = DeathAnim::default();
            **animator = KnightAnimator::default();
            **flash = HitFlash::default();
            move_target.position = None;
        }

        self.target_state.hovered = None;
        self.target_state.targeted = None;
        self.respawn_event.send(RespawnEnemies);
    }
}

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
                    spawn_pause_button(panel, PauseMenuAction::Quit, "QUIT");
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
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|parent| {
            parent.spawn((
                DeathText,
                Text::new("YOU DIED"),
                TextFont {
                    font_size: 64.0,
                    ..default()
                },
                TextColor(Color::srgba(0.7, 0.10, 0.08, 0.0)),
            ));
        });
}

fn init_bonfire_position(
    bonfires: Query<&Transform, With<Bonfire>>,
    mut last_bonfire: ResMut<LastBonfirePosition>,
) {
    if let Some(tf) = bonfires.iter().next() {
        last_bonfire.0 = tf.translation;
    }
}

#[derive(SystemParam)]
struct DeathSequenceContext<'w, 's> {
    time: Res<'w, Time>,
    respawn_state: ResMut<'w, DeathRespawnState>,
    death_screen_bg: Query<'w, 's, &'static mut BackgroundColor, With<DeathScreen>>,
    death_text: Query<'w, 's, &'static mut TextColor, With<DeathText>>,
    restart: RestartContext<'w, 's>,
}

fn death_respawn_sequence(mut ctx: DeathSequenceContext<'_, '_>) {
    let delta = ctx.time.delta_secs();

    match ctx.respawn_state.phase {
        DeathPhase::Alive => {
            let Some(player) = ctx.restart.player.as_ref() else {
                return;
            };
            let (_, ref hp, _, _, _, _, _, _, _, ref death_anim, _, _) = **player;
            let _ = death_anim;
            if hp.is_dead() && !ctx.restart.game_over.0 {
                ctx.restart.game_over.0 = true;
                ctx.restart.pause_menu.close();
                ctx.respawn_state.phase = DeathPhase::Dying;
            }
        }
        DeathPhase::Dying => {
            let Some(player) = ctx.restart.player.as_ref() else {
                return;
            };
            let (_, _, _, _, _, _, _, _, _, ref death_anim, _, _) = **player;
            if death_anim.active && death_anim.timer.finished() {
                for mut vis in &mut ctx.restart.death_screen {
                    *vis = Visibility::Visible;
                }
                ctx.respawn_state.phase = DeathPhase::FadeOut(0.0);
            }
        }
        DeathPhase::FadeOut(ref mut elapsed) => {
            *elapsed += delta;
            let text_alpha = if *elapsed < DEATH_TEXT_DELAY {
                0.0
            } else {
                ((*elapsed - DEATH_TEXT_DELAY) / 0.5).clamp(0.0, 1.0)
            };
            for mut color in &mut ctx.death_text {
                *color = TextColor(Color::srgba(0.7, 0.10, 0.08, text_alpha));
            }
            let bg_alpha = (*elapsed / DEATH_FADE_OUT_DURATION).clamp(0.0, 1.0);
            for mut bg in &mut ctx.death_screen_bg {
                *bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, bg_alpha));
            }
            if *elapsed >= DEATH_FADE_OUT_DURATION {
                ctx.respawn_state.phase = DeathPhase::Respawn;
            }
        }
        DeathPhase::Respawn => {
            for mut color in &mut ctx.death_text {
                *color = TextColor(Color::srgba(0.7, 0.10, 0.08, 0.0));
            }
            let bonfire_pos = ctx.restart.last_bonfire.0;
            ctx.restart.respawn_at_bonfire(bonfire_pos);
            ctx.respawn_state.phase = DeathPhase::FadeIn(0.0);
        }
        DeathPhase::FadeIn(ref mut elapsed) => {
            *elapsed += delta;
            let alpha = 1.0 - (*elapsed / DEATH_FADE_IN_DURATION).clamp(0.0, 1.0);
            for mut bg in &mut ctx.death_screen_bg {
                *bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, alpha));
            }
            if *elapsed >= DEATH_FADE_IN_DURATION {
                for mut vis in &mut ctx.restart.death_screen {
                    *vis = Visibility::Hidden;
                }
                ctx.respawn_state.phase = DeathPhase::Alive;
            }
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

    let navigate_up =
        keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW);
    let navigate_down =
        keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS);

    let stick_y = gamepads.iter().fold(0.0_f32, |best, gamepad| {
        let dpad = gamepad.dpad().y;
        let stick = gamepad.left_stick().y;
        let candidate = if dpad.abs() >= stick.abs() {
            dpad
        } else {
            stick
        };
        if candidate.abs() > best.abs() {
            candidate
        } else {
            best
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
    mut restart: RestartContext<'_, '_>,
) {
    if !restart.pause_menu.open {
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

    let action = restart.pause_menu.selected;
    restart.execute_pause_menu_action(action);
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
    mut buttons: PauseMenuButtonInteractions<'_, '_>,
    mut restart: RestartContext<'_, '_>,
) {
    if !restart.pause_menu.open {
        return;
    }

    let mut action_to_execute = None;
    for (interaction, button) in &mut buttons {
        match interaction {
            Interaction::Hovered => {
                restart.pause_menu.selected = button.0;
            }
            Interaction::Pressed => {
                restart.pause_menu.selected = button.0;
                action_to_execute = Some(button.0);
            }
            Interaction::None => {}
        }
    }

    if let Some(action) = action_to_execute {
        restart.execute_pause_menu_action(action);
    }
}

