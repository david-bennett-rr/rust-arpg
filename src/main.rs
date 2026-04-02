use bevy::prelude::*;
use bevy::render::{
    settings::{Backends, InstanceFlags, PowerPreference, WgpuSettings},
    RenderPlugin,
};
use bevy::window::PresentMode;

mod camera;
mod combat;
#[cfg(debug_assertions)]
mod debug;
mod enemy;
mod hud;
mod player;
mod rng;
mod targeting;
mod world;

const LOG_FILTER: &str = "wgpu=error,wgpu_hal=error,gilrs=error,naga=warn";

fn main() {
    normalize_rust_log_env();

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(bevy::log::LogPlugin {
                filter: LOG_FILTER.into(),
                level: bevy::log::Level::WARN,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Rust ARPG".to_string(),
                    resolution: (1280.0, 720.0).into(),
                    present_mode: default_present_mode(),
                    ..default()
                }),
                ..default()
            })
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: Some(preferred_backends()),
                    power_preference: PowerPreference::HighPerformance,
                    instance_flags: preferred_instance_flags(),
                    ..default()
                }
                .into(),
                ..default()
            }),
    )
    .add_plugins((
        camera::CameraPlugin,
        combat::CombatPlugin,
        world::WorldPlugin,
        targeting::TargetingPlugin,
        player::PlayerPlugin,
        enemy::EnemyPlugin,
        hud::HudPlugin,
    ));

    #[cfg(debug_assertions)]
    app.add_plugins(debug::DebugPlugin);

    app.run();
}

fn normalize_rust_log_env() {
    let generic_shell_filter = matches!(std::env::var("RUST_LOG").as_deref(), Ok("warn") | Ok(""));
    if generic_shell_filter || std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: we set the process env once during single-threaded startup,
        // before Bevy initializes logging or worker threads.
        unsafe {
            std::env::set_var("RUST_LOG", format!("warn,{LOG_FILTER}"));
        }
    }
}

fn default_present_mode() -> PresentMode {
    if cfg!(debug_assertions) {
        PresentMode::Immediate
    } else {
        PresentMode::Fifo
    }
}

fn preferred_backends() -> Backends {
    #[cfg(target_os = "windows")]
    {
        Backends::PRIMARY
    }

    #[cfg(not(target_os = "windows"))]
    {
        Backends::all()
    }
}

fn preferred_instance_flags() -> InstanceFlags {
    if cfg!(debug_assertions) {
        InstanceFlags::empty().with_env()
    } else {
        InstanceFlags::default().with_env()
    }
}
