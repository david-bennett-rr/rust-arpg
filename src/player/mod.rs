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
            .add_systems(Startup, spawn::spawn_player)
            .add_systems(
                Update,
                (
                    systems::update_controller_move_state,
                    systems::handle_left_click,
                    systems::handle_controller_targeting,
                    systems::chase_and_attack_target,
                    systems::trigger_dodge,
                    systems::update_dodge,
                    systems::regen_stamina,
                    systems::move_player_with_controller,
                    systems::move_player,
                    animation::animate_knight,
                )
                    .chain()
                    .run_if(game_running)
                    .in_set(PlayerSet::Update),
            )
            .add_systems(Update, animation::animate_death.after(PlayerSet::Update));
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct MoveTarget {
    pub position: Option<Vec2>,
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

#[derive(Component, Default)]
pub struct PlayerCombat {
    pub swing_id: u32,
    pub strike: f32,
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
    pub mana: f32,
    pub max_mana: f32,
    regen_delay: Timer,
}

impl Default for PlayerStats {
    fn default() -> Self {
        let mut delay = Timer::from_seconds(STAMINA_REGEN_DELAY, TimerMode::Once);
        delay.tick(std::time::Duration::from_secs_f32(STAMINA_REGEN_DELAY));
        Self {
            stamina: MAX_STAMINA,
            max_stamina: MAX_STAMINA,
            mana: MAX_MANA,
            max_mana: MAX_MANA,
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
const ARRIVE_THRESHOLD: f32 = 0.15;
pub const PLAYER_MAX_HP: i32 = 20;
const ATTACK_RANGE: f32 = 2.3;
const ATTACK_WINDUP: f32 = 0.14;
const DODGE_DURATION: f32 = 0.38;
const DODGE_COOLDOWN: f32 = 0.8;
const DODGE_DISTANCE: f32 = 6.4;

const MAX_STAMINA: f32 = 60.0;
const MAX_MANA: f32 = 40.0;
const STAMINA_REGEN: f32 = 18.0;
const STAMINA_REGEN_DELAY: f32 = 0.6;
const ATTACK_STAMINA_COST: f32 = 22.0;
const DODGE_STAMINA_COST: f32 = 12.0;

pub fn visual_forward(transform: &Transform) -> Vec3 {
    transform.rotation * Vec3::Z
}

fn dodge_motion_curve(progress: f32) -> f32 {
    let clamped = progress.clamp(0.0, 1.0);
    1.0 - (1.0 - clamped).powi(3)
}
