use bevy::prelude::*;

use crate::hud::PauseMenuState;
use crate::rng::SplitMix64;

#[derive(Resource, Default)]
pub struct GameOver(pub bool);

pub fn game_running(game_over: Res<GameOver>, pause_menu: Option<Res<PauseMenuState>>) -> bool {
    !game_over.0 && pause_menu.is_none_or(|pause_menu| !pause_menu.open)
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameOver>()
            .init_resource::<DamageRng>()
            .add_systems(
                Update,
                (tick_hit_flashes, tick_stun_meters, update_hit_flash_tints).chain(),
            );
    }
}

#[derive(Component, Clone, Copy)]
pub struct HitPoints {
    pub current: i32,
    pub max: i32,
}

impl HitPoints {
    pub const fn new(max: i32) -> Self {
        Self { current: max, max }
    }

    pub fn apply_damage(&mut self, damage: i32) -> i32 {
        let applied = damage.max(0).min(self.current);
        self.current -= applied;
        applied
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0
    }

    pub fn heal_to_full(&mut self) {
        self.current = self.max;
    }

    pub fn fraction(&self) -> f32 {
        if self.max <= 0 {
            return 0.0;
        }
        self.current as f32 / self.max as f32
    }
}

#[derive(Component)]
pub struct StunMeter {
    pub current: f32,
    pub max: f32,
    pub stunned: bool,
    pub stun_duration: f32,
    pub stun_timer: f32,
    pub refill_rate: f32,
}

impl StunMeter {
    pub fn new(max: f32, stun_duration: f32, refill_rate: f32) -> Self {
        Self {
            current: max,
            max,
            stunned: false,
            stun_duration,
            stun_timer: 0.0,
            refill_rate,
        }
    }

    pub fn apply_stun_damage(&mut self, amount: f32) {
        if self.stunned {
            return;
        }
        self.current = (self.current - amount).max(0.0);
        if self.current <= 0.0 {
            self.stunned = true;
            self.stun_timer = self.stun_duration;
        }
    }

    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            return 0.0;
        }
        self.current / self.max
    }
}

#[derive(Component)]
pub struct HitFlash {
    timer: Timer,
    active: bool,
    dirty: bool,
}

impl Default for HitFlash {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.12, TimerMode::Once),
            active: false,
            dirty: false,
        }
    }
}

impl HitFlash {
    pub fn trigger(&mut self) {
        self.timer.reset();
        self.active = true;
        self.dirty = true;
    }

    pub fn amount(&self) -> f32 {
        if !self.active {
            return 0.0;
        }

        let duration = self.timer.duration().as_secs_f32().max(0.0001);
        (1.0 - self.timer.elapsed_secs() / duration).clamp(0.0, 1.0)
    }
}

#[derive(Component, Clone, Copy)]
pub struct FlashTint {
    pub owner: Entity,
    pub base_srgb: Vec3,
}

fn tick_hit_flashes(time: Res<Time>, mut flashes: Query<&mut HitFlash>) {
    for mut flash in &mut flashes {
        if !flash.dirty {
            continue;
        }

        if !flash.active {
            // Base color was restored last frame; stop processing.
            flash.dirty = false;
            continue;
        }

        flash.timer.tick(time.delta());
        if flash.timer.finished() {
            flash.active = false;
        }
    }
}

fn tick_stun_meters(time: Res<Time>, mut meters: Query<&mut StunMeter>) {
    let delta = time.delta_secs();
    for mut meter in &mut meters {
        if meter.stunned {
            meter.stun_timer -= delta;
            if meter.stun_timer <= 0.0 {
                meter.stunned = false;
                meter.stun_timer = 0.0;
                meter.current = 0.0; // starts empty, refills
            }
        } else if meter.current < meter.max {
            meter.current = (meter.current + meter.refill_rate * delta).min(meter.max);
        }
    }
}

fn update_hit_flash_tints(
    flashes: Query<&HitFlash>,
    tint_targets: Query<(&FlashTint, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (tint, material_handle) in &tint_targets {
        let Ok(flash) = flashes.get(tint.owner) else {
            continue;
        };
        if !flash.dirty {
            continue;
        }
        let amount = flash.amount();
        let Some(material) = materials.get_mut(&material_handle.0) else {
            continue;
        };

        material.base_color = flash_color(tint.base_srgb, amount);
    }
}

fn flash_color(base_srgb: Vec3, amount: f32) -> Color {
    let hit_red = Vec3::new(1.0, 0.10, 0.10);
    let blend = base_srgb + (hit_red - base_srgb) * (amount * 0.88);
    Color::srgb(blend.x, blend.y, blend.z)
}

#[derive(Resource, Default)]
pub struct DamageRng(SplitMix64);

impl DamageRng {
    pub fn roll_1d5(&mut self) -> i32 {
        self.0.next_usize(5) as i32 + 1
    }
}

pub fn smoothstep01(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_rng_rolls_stay_in_range() {
        let mut rng = DamageRng(SplitMix64::new(123));

        for _ in 0..32 {
            assert!((1..=5).contains(&rng.roll_1d5()));
        }
    }

    #[test]
    fn hit_points_damage_clamps_to_remaining_health() {
        let mut hp = HitPoints {
            current: 3,
            max: 10,
        };

        assert_eq!(hp.apply_damage(8), 3);
        assert_eq!(hp.current, 0);
        assert!(hp.is_dead());
    }
}
