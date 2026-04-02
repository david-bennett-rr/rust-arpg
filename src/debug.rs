use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::camera::MainCamera;
use crate::player::{MoveTarget, Player};

const FPS_DIP_THRESHOLD: f64 = 20.0;
const FRAME_TIME_SPIKE_MS: f64 = 55.0; // ~18fps; only flag real stalls, not normal debug overhead
const DIP_LOG_COOLDOWN_SECS: f32 = 2.0;

/// How often we write a snapshot line to the debug log.
const LOG_INTERVAL_SECS: f32 = 1.0;
/// Max log file size before we rotate (1 MB).
const MAX_LOG_BYTES: u64 = 1_024 * 1_024;
/// How many rotated files to keep (debug.log, debug.1.log).
const MAX_ROTATED_FILES: u32 = 1;

fn log_path() -> PathBuf {
    PathBuf::from("debug.log")
}

fn rotated_path(i: u32) -> PathBuf {
    PathBuf::from(format!("debug.{i}.log"))
}

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin)
            .insert_resource(PerfLog {
                worst_fps: f64::MAX,
                worst_frame_ms: 0.0,
                dip_count: 0,
                cooldown: Timer::from_seconds(DIP_LOG_COOLDOWN_SECS, TimerMode::Once),
            })
            .insert_resource(FileLogger::new())
            .add_systems(Startup, (spawn_debug_overlay, log_render_adapter_info))
            .add_systems(
                Update,
                (update_debug_overlay, log_perf_dips, write_debug_to_file).chain(),
            );
    }
}

#[derive(Resource)]
struct PerfLog {
    worst_fps: f64,
    worst_frame_ms: f64,
    dip_count: u32,
    cooldown: Timer,
}

#[derive(Resource)]
struct FileLogger {
    sender: Sender<String>,
    timer: Timer,
    frame_count: u64,
}

impl FileLogger {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || run_log_worker(receiver));

        Self {
            sender,
            timer: Timer::from_seconds(LOG_INTERVAL_SECS, TimerMode::Repeating),
            frame_count: 0,
        }
    }

    fn write_line(&self, line: &str) {
        let _ = self.sender.send(line.to_owned());
    }
}

struct LogWriter {
    file: Option<BufWriter<File>>,
    bytes_written: u64,
}

impl LogWriter {
    fn new() -> Self {
        let file = match File::create(log_path()) {
            Ok(file) => Some(BufWriter::new(file)),
            Err(err) => {
                error!("failed to create debug log: {err}");
                None
            }
        };

        Self {
            file,
            bytes_written: 0,
        }
    }

    fn rotate_if_needed(&mut self, next_line_bytes: u64) {
        if self.bytes_written + next_line_bytes <= MAX_LOG_BYTES {
            return;
        }

        self.flush();

        for i in (MAX_ROTATED_FILES..MAX_ROTATED_FILES + 5).rev() {
            let _ = fs::remove_file(rotated_path(i));
        }
        for i in (1..MAX_ROTATED_FILES).rev() {
            let _ = fs::rename(rotated_path(i), rotated_path(i + 1));
        }
        let _ = fs::rename(log_path(), rotated_path(1));

        self.file = match File::create(log_path()) {
            Ok(file) => Some(BufWriter::new(file)),
            Err(err) => {
                error!("failed to rotate debug log: {err}");
                None
            }
        };
        self.bytes_written = 0;
    }

    fn write_line(&mut self, line: &str) {
        let line_bytes = line.len() as u64 + 1;
        self.rotate_if_needed(line_bytes);

        let Some(file) = self.file.as_mut() else {
            return;
        };

        if writeln!(file, "{line}").is_ok() {
            self.bytes_written += line_bytes;
        }
    }

    fn flush(&mut self) {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
    }
}

fn run_log_worker(receiver: Receiver<String>) {
    let mut writer = LogWriter::new();
    while let Ok(line) = receiver.recv() {
        writer.write_line(&line);
    }
    writer.flush();
}

fn log_render_adapter_info(adapter_info: Res<RenderAdapterInfo>, file_logger: Res<FileLogger>) {
    file_logger.write_line(&format!("[GPU] {:?}", &**adapter_info));
}

#[derive(Component)]
struct DebugOverlayText;

fn spawn_debug_overlay(mut commands: Commands) {
    commands.spawn((
        DebugOverlayText,
        Text::new(""),
        TextFont {
            font_size: 11.0,
            ..default()
        },
        TextColor(Color::srgb(0.6, 0.6, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(6.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
}

fn update_debug_overlay(
    diagnostics: Res<DiagnosticsStore>,
    mut text: Single<&mut Text, With<DebugOverlayText>>,
) {
    let frame_time_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let fps = if frame_time_ms > 0.0 {
        1000.0 / frame_time_ms
    } else {
        0.0
    };
    text.0 = format!("{fps:.0} fps");
}

fn log_perf_dips(mut perf_log: ResMut<PerfLog>, time: Res<Time>, file_logger: Res<FileLogger>) {
    perf_log.cooldown.tick(time.delta());

    let delta_secs = time.delta_secs_f64();
    if delta_secs == 0.0 {
        return;
    }

    let frame_ms = delta_secs * 1000.0;
    let fps = 1.0 / delta_secs;

    if frame_ms > perf_log.worst_frame_ms {
        perf_log.worst_frame_ms = frame_ms;
    }
    if fps < perf_log.worst_fps {
        perf_log.worst_fps = fps;
    }

    if (fps < FPS_DIP_THRESHOLD || frame_ms > FRAME_TIME_SPIKE_MS) && perf_log.cooldown.finished() {
        perf_log.dip_count += 1;
        perf_log.cooldown.reset();
        let msg = format!(
            "PERF DIP #{}: fps={:.1}, frame={:.1}ms",
            perf_log.dip_count, fps, frame_ms
        );
        warn!("{msg}");
        file_logger.write_line(&format!("[PERF] {msg}"));
    }
}

fn write_debug_to_file(
    mut file_logger: ResMut<FileLogger>,
    time: Res<Time>,
    perf_log: Res<PerfLog>,
    entities: Query<Entity>,
    player: Option<Single<(&Transform, &MoveTarget), With<Player>>>,
    camera: Option<Single<(&Transform, &Projection), With<MainCamera>>>,
) {
    file_logger.frame_count += 1;
    file_logger.timer.tick(time.delta());
    if !file_logger.timer.just_finished() {
        return;
    }

    let entity_count = entities.iter().count();
    let elapsed = time.elapsed_secs();
    let delta_secs = time.delta_secs_f64();
    let fps = if delta_secs > 0.0 {
        1.0 / delta_secs
    } else {
        0.0
    };
    let frame_ms = delta_secs * 1000.0;

    let player_info = match player {
        Some(ref p) => {
            let (tf, mt) = &**p;
            let pos = tf.translation;
            let target = match mt.position {
                Some(t) => format!("({:.1},{:.1})", t.x, t.y),
                None => "none".into(),
            };
            format!(
                "pos=({:.1},{:.1},{:.1}) target={target}",
                pos.x, pos.y, pos.z
            )
        }
        None => "no player".into(),
    };

    let cam_info = match camera {
        Some(ref c) => {
            let (tf, proj) = &**c;
            let pos = tf.translation;
            let zoom = match proj {
                Projection::Orthographic(orthographic) => orthographic.scale,
                Projection::Perspective(_) => 0.0,
            };
            format!(
                "pos=({:.1},{:.1},{:.1}) zoom={zoom:.2}",
                pos.x, pos.y, pos.z
            )
        }
        None => "no camera".into(),
    };

    let line = format!(
        "[{elapsed:>8.1}s] frame={} fps={fps:.0} dt={frame_ms:.1}ms ent={} \
         worst_fps={:.0} worst_dt={:.1}ms dips={} | player: {player_info} | cam: {cam_info}",
        file_logger.frame_count,
        entity_count,
        if perf_log.worst_fps == f64::MAX {
            0.0
        } else {
            perf_log.worst_fps
        },
        perf_log.worst_frame_ms,
        perf_log.dip_count,
    );
    file_logger.write_line(&line);
}
