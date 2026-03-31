use bevy::prelude::*;
use bevy::render::{
    RenderPlugin,
    settings::{Backends, InstanceFlags, PowerPreference, WgpuSettings},
};
use bevy::window::PresentMode;

mod camera;
mod combat;
#[cfg(debug_assertions)]
mod debug;
mod enemy;
mod player;
mod world;

fn main() {
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
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
        player::PlayerPlugin,
        enemy::EnemyPlugin,
    ));

    #[cfg(debug_assertions)]
    app.add_plugins(debug::DebugPlugin);

    app.run();
}

fn default_present_mode() -> PresentMode {
    if cfg!(debug_assertions) {
        PresentMode::AutoNoVsync
    } else {
        PresentMode::AutoVsync
    }
}

fn preferred_backends() -> Backends {
    #[cfg(target_os = "windows")]
    {
        Backends::DX12 | Backends::VULKAN
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
