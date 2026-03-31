use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*, render::camera::ScalingMode};

use crate::player::Player;

const CAMERA_OFFSET: Vec3 = Vec3::new(18.0, 20.0, 18.0);
const CAMERA_LOOK_AT_HEIGHT: f32 = 1.2;
const CAMERA_VIEWPORT_HEIGHT: f32 = 28.0;
const CAMERA_MIN_ZOOM: f32 = 0.45;
const CAMERA_MAX_ZOOM: f32 = 1.8;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, (camera_follow, camera_zoom));
    }
}

#[derive(Component)]
pub struct MainCamera;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Tonemapping::AcesFitted,
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: CAMERA_VIEWPORT_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        MainCamera,
        Transform::from_translation(CAMERA_OFFSET).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn camera_follow(
    player_tf: Single<&Transform, (With<Player>, Without<MainCamera>)>,
    mut cam_tf: Single<&mut Transform, With<MainCamera>>,
) {
    let focus = player_tf.translation + Vec3::Y * CAMERA_LOOK_AT_HEIGHT;
    **cam_tf = Transform::from_translation(player_tf.translation + CAMERA_OFFSET)
        .looking_at(focus, Vec3::Y);
}

fn camera_zoom(
    mut scroll_events: EventReader<bevy::input::mouse::MouseWheel>,
    mut projection: Single<&mut Projection, With<MainCamera>>,
) {
    for event in scroll_events.read() {
        let Projection::Orthographic(orthographic) = &mut **projection else {
            continue;
        };

        orthographic.scale =
            (orthographic.scale - event.y * 0.08).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
    }
}
