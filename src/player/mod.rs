mod animation;
mod spawn;
mod systems;

use bevy::prelude::*;

use crate::combat::game_running;

pub struct PlayerPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum PlayerSet {
    Update,
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, PlayerSet::Update)
            .add_systems(PostStartup, spawn::spawn_player)
            .add_systems(
                Update,
                (
                    systems::update_controller_move_state,
                    systems::handle_left_click,
                    systems::handle_controller_targeting,
                    systems::chase_and_attack_target,
                    systems::update_attack_lunge,
                    systems::trigger_dodge,
                    systems::update_dodge,
                    systems::update_run_state,
                    systems::use_healing_flask,
                    systems::rest_at_bonfire,
                    systems::regen_stamina,
                    systems::move_player_with_controller,
                    systems::move_player,
                    animation::animate_knight,
                )
                    .chain()
                    .run_if(game_running)
                    .in_set(PlayerSet::Update),
            )
            .add_systems(
                Update,
                (animation::animate_rest, animation::animate_death).after(PlayerSet::Update),
            );
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct MoveTarget {
    pub position: Option<Vec2>,
}

#[derive(Component, Default)]
pub struct RunState {
    pub active: bool,
}

#[derive(Component, Default)]
pub struct ControllerMove {
    pub input: Vec2,
}

impl ControllerMove {
    pub fn active(&self) -> bool {
        self.input.length_squared() > 0.0001
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum AttackKind {
    #[default]
    Light,
    Heavy,
}

impl AttackKind {
    pub fn windup(self) -> f32 {
        match self {
            Self::Light => LIGHT_WINDUP,
            Self::Heavy => HEAVY_WINDUP,
        }
    }

    pub fn swing_duration(self) -> f32 {
        match self {
            Self::Light => LIGHT_SWING_DURATION,
            Self::Heavy => HEAVY_SWING_DURATION,
        }
    }

    pub fn stamina_cost(self) -> f32 {
        match self {
            Self::Light => LIGHT_STAMINA_COST,
            Self::Heavy => HEAVY_STAMINA_COST,
        }
    }
}

#[derive(Component, Default)]
pub struct PlayerCombat {
    pub swing_id: u32,
    pub strike: f32,
    pub attack_kind: AttackKind,
}

#[derive(Component)]
pub struct Dodge {
    timer: Timer,
    cooldown: Timer,
    direction: Vec3,
    pub active: bool,
}

impl Default for Dodge {
    fn default() -> Self {
        let mut cooldown = Timer::from_seconds(DODGE_COOLDOWN, TimerMode::Once);
        cooldown.tick(std::time::Duration::from_secs_f32(DODGE_COOLDOWN));
        let mut timer = Timer::from_seconds(DODGE_DURATION, TimerMode::Once);
        timer.tick(std::time::Duration::from_secs_f32(DODGE_DURATION));
        Self {
            timer,
            cooldown,
            direction: Vec3::ZERO,
            active: false,
        }
    }
}

#[derive(Component)]
pub struct PlayerStats {
    pub stamina: f32,
    pub max_stamina: f32,
    regen_delay: Timer,
}

impl Default for PlayerStats {
    fn default() -> Self {
        let mut delay = Timer::from_seconds(STAMINA_REGEN_DELAY, TimerMode::Once);
        delay.tick(std::time::Duration::from_secs_f32(STAMINA_REGEN_DELAY));
        Self {
            stamina: MAX_STAMINA,
            max_stamina: MAX_STAMINA,
            regen_delay: delay,
        }
    }
}

impl PlayerStats {
    pub fn spend_stamina(&mut self, amount: f32) -> bool {
        if self.stamina >= amount {
            self.stamina -= amount;
            self.regen_delay.reset();
            true
        } else {
            false
        }
    }
}

#[derive(Component)]
pub struct AttackLunge {
    pub(super) start: Vec2,
    pub(super) end: Vec2,
    pub(super) timer: Timer,
}

impl Default for AttackLunge {
    fn default() -> Self {
        let mut timer = Timer::from_seconds(ATTACK_LUNGE_DURATION, TimerMode::Once);
        timer.tick(std::time::Duration::from_secs_f32(ATTACK_LUNGE_DURATION));
        Self {
            start: Vec2::ZERO,
            end: Vec2::ZERO,
            timer,
        }
    }
}

impl AttackLunge {
    pub fn active(&self) -> bool {
        !self.timer.finished()
    }

    pub fn progress(&self) -> f32 {
        let duration = self.timer.duration().as_secs_f32().max(0.001);
        (self.timer.elapsed_secs() / duration).clamp(0.0, 1.0)
    }
}

const FLASK_CHARGES: i32 = 3;
const FLASK_HEAL: i32 = 20;
const FLASK_COOLDOWN: f32 = 0.8;

#[derive(Component)]
pub struct HealingFlask {
    pub charges: i32,
    pub heal_amount: i32,
    cooldown: Timer,
}

impl Default for HealingFlask {
    fn default() -> Self {
        let mut cooldown = Timer::from_seconds(FLASK_COOLDOWN, TimerMode::Once);
        cooldown.tick(std::time::Duration::from_secs_f32(FLASK_COOLDOWN));
        Self {
            charges: FLASK_CHARGES,
            heal_amount: FLASK_HEAL,
            cooldown,
        }
    }
}

const DEATH_ANIM_DURATION: f32 = 1.2;

#[derive(Component)]
pub struct DeathAnim {
    pub timer: Timer,
    pub active: bool,
}

impl Default for DeathAnim {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(DEATH_ANIM_DURATION, TimerMode::Once),
            active: false,
        }
    }
}

#[derive(Component)]
pub(crate) struct KnightAnimator {
    pub(crate) walk_phase: f32,
    pub(crate) swing_timer: Timer,
}

impl Default for KnightAnimator {
    fn default() -> Self {
        Self {
            walk_phase: 0.0,
            swing_timer: Timer::from_seconds(0.55, TimerMode::Once),
        }
    }
}

impl KnightAnimator {
    pub(super) fn cancel_swing(&mut self) {
        self.swing_timer.set_elapsed(self.swing_timer.duration());
    }
}

#[derive(Component, Clone, Copy)]
pub(super) struct JointRest {
    translation: Vec3,
    rotation: Quat,
}

impl JointRest {
    pub(super) fn new(translation: Vec3, rotation: Quat) -> Self {
        Self {
            translation,
            rotation,
        }
    }
}

#[derive(Component, Clone, Copy)]
pub(super) enum KnightJoint {
    Hips,
    Chest,
    Head,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Sword,
}

pub const PLAYER_COLLISION_RADIUS: f32 = 0.78;
const MOVE_SPEED: f32 = 8.0;
const RUN_SPEED_MULTIPLIER: f32 = 1.5;
const ARRIVE_THRESHOLD: f32 = 0.15;
pub const PLAYER_MAX_HP: i32 = 20;
const ATTACK_RANGE: f32 = 2.3;
const ATTACK_LUNGE_RANGE: f32 = 3.5;
const ATTACK_LUNGE_STOP: f32 = 1.6;
const ATTACK_LUNGE_DURATION: f32 = 0.12;
const DODGE_DURATION: f32 = 0.54;
const DODGE_COOLDOWN: f32 = 0.8;
const DODGE_DISTANCE: f32 = 8.0;

// Light attack (RB / left-click)
const LIGHT_WINDUP: f32 = 0.08;
const LIGHT_SWING_DURATION: f32 = 0.40;
const LIGHT_STAMINA_COST: f32 = 15.0;

// Heavy attack (RT / right-click)
const HEAVY_WINDUP: f32 = 0.24;
const HEAVY_SWING_DURATION: f32 = 0.70;
const HEAVY_STAMINA_COST: f32 = 30.0;

const MAX_STAMINA: f32 = 60.0;
const STAMINA_REGEN: f32 = 18.0;
const STAMINA_REGEN_DELAY: f32 = 0.6;
const DODGE_STAMINA_COST: f32 = 12.0;
const RUN_STAMINA_DRAIN: f32 = 10.0;

pub fn visual_forward(transform: &Transform) -> Vec3 {
    transform.rotation * Vec3::Z
}

fn dodge_motion_curve(progress: f32) -> f32 {
    let clamped = progress.clamp(0.0, 1.0);
    // Strong ease-out so the roll carries farther, then settles with a visible slowdown.
    1.0 - (1.0 - clamped).powi(4)
}
