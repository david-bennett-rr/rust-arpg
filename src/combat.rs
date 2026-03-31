use bevy::prelude::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tick_hit_flashes, update_hit_flash_tints).chain());
    }
}

#[derive(Component, Clone, Copy)]
pub struct HitPoints {
    pub current: i32,
}

impl HitPoints {
    pub const fn new(max: i32) -> Self {
        Self { current: max }
    }

    pub fn apply_damage(&mut self, damage: i32) -> i32 {
        let applied = damage.max(0).min(self.current);
        self.current -= applied;
        applied
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0
    }
}

#[derive(Component)]
pub struct HitFlash {
    timer: Timer,
    active: bool,
}

impl Default for HitFlash {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.12, TimerMode::Once),
            active: false,
        }
    }
}

impl HitFlash {
    pub fn trigger(&mut self) {
        self.timer.reset();
        self.active = true;
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
        if !flash.active {
            continue;
        }

        flash.timer.tick(time.delta());
        if flash.timer.finished() {
            flash.active = false;
        }
    }
}

fn update_hit_flash_tints(
    flashes: Query<&HitFlash>,
    tint_targets: Query<(&FlashTint, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (tint, material_handle) in &tint_targets {
        let amount = flashes.get(tint.owner).map(HitFlash::amount).unwrap_or(0.0);
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
