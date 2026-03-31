use bevy::{input::gamepad::Gamepad, prelude::*};
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::targeting::{TargetState, Targetable};
use crate::world::tilemap::{clamp_ground_target_to_arena, clamp_translation_to_arena};

use super::{
    visual_forward, ControllerMove, Dodge, KnightAnimator, MoveTarget, Player, PlayerCombat,
    PlayerStats, ARRIVE_THRESHOLD, ATTACK_RANGE, ATTACK_STAMINA_COST, ATTACK_WINDUP, DODGE_SPEED,
    DODGE_STAMINA_COST, MOVE_SPEED, PLAYER_COLLISION_RADIUS,
};

type DodgePlayer<'w> = Single<
    'w,
    (
        &'static Transform,
        &'static mut Dodge,
        &'static mut MoveTarget,
        &'static mut PlayerStats,
        &'static mut KnightAnimator,
        &'static mut PlayerCombat,
    ),
    With<Player>,
>;

const MOVE_DEADZONE: f32 = 0.18;
const TARGET_SWITCH_DEADZONE: f32 = 0.72;
const TARGET_DIRECTION_DOT: f32 = 0.2;

#[derive(Default)]
pub(super) struct AttackIntent {
    pending_strike: bool,
    holding: bool,
    windup: f32,
}

impl AttackIntent {
    fn clear(&mut self) {
        self.pending_strike = false;
        self.holding = false;
        self.windup = 0.0;
    }
}

pub(super) fn update_controller_move_state(
    gamepads: Query<&Gamepad>,
    camera_query: Option<Single<&Transform, With<MainCamera>>>,
    mut player: Single<&mut ControllerMove, With<Player>>,
) {
    let Some(camera_tf) = camera_query else {
        player.input = Vec2::ZERO;
        return;
    };

    player.input =
        strongest_stick_direction(&gamepads, |gamepad| gamepad.left_stick(), *camera_tf, MOVE_DEADZONE);
}

pub(super) fn handle_left_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    camera_query: Option<Single<(&Camera, &GlobalTransform), With<MainCamera>>>,
    mut player: Single<&mut MoveTarget, With<Player>>,
    mut target_state: ResMut<TargetState>,
) {
    // Click on enemy: set target
    if mouse_buttons.just_pressed(MouseButton::Left) {
        if let Some(hovered) = target_state.hovered {
            target_state.targeted = Some(hovered);
            player.position = None;
            return;
        }

        target_state.targeted = None;
    }

    // Hold on ground: continuously move toward cursor
    if !mouse_buttons.pressed(MouseButton::Left) {
        return;
    }

    let Some(window) = window else {
        return;
    };
    let Some(camera_query) = camera_query else {
        return;
    };

    // Don't override movement while chasing a target
    if target_state.targeted.is_some() {
        return;
    }

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let (camera, cam_transform) = *camera_query;

    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
        return;
    };
    let Some(distance) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };

    let point = ray.get_point(distance);
    player.position = Some(clamp_ground_target_to_arena(
        Vec2::new(point.x, point.z),
        PLAYER_COLLISION_RADIUS,
    ));
}

pub(super) fn handle_controller_targeting(
    gamepads: Query<&Gamepad>,
    camera_query: Option<Single<&Transform, With<MainCamera>>>,
    controller_move: Single<&ControllerMove, With<Player>>,
    player: Single<&Transform, With<Player>>,
    targetables: Query<(Entity, &GlobalTransform), With<Targetable>>,
    mut target_state: ResMut<TargetState>,
    mut right_stick_released: Local<bool>,
) {
    if let Some(targeted) = target_state.targeted
        && targetables.get(targeted).is_err()
    {
        target_state.targeted = None;
    }

    if let Some(camera_tf) = camera_query {
        let right_input = strongest_stick_direction(
            &gamepads,
            |gamepad| gamepad.right_stick(),
            *camera_tf,
            MOVE_DEADZONE,
        );
        let right_active = right_input.length_squared() >= TARGET_SWITCH_DEADZONE * TARGET_SWITCH_DEADZONE;

        if right_active && *right_stick_released {
            let excluded = target_state.targeted;
            if let Some(target) = find_directional_target(
                player.translation,
                right_input,
                excluded,
                &targetables,
            )
            .or_else(|| find_directional_target(player.translation, right_input, None, &targetables))
            {
                target_state.targeted = Some(target);
            }
            *right_stick_released = false;
        } else if !right_active {
            *right_stick_released = true;
        }
    } else {
        *right_stick_released = true;
    }

    if controller_move.active() {
        let direction = controller_move.input;
        let keep_current = target_state
            .targeted
            .and_then(|entity| targetables.get(entity).ok())
            .is_some_and(|(_, transform)| {
                target_in_direction(player.translation, direction, transform.translation())
            });

        if !keep_current {
            target_state.targeted =
                find_directional_target(player.translation, direction, None, &targetables);
        }
    }
}

pub(super) fn chase_and_attack_target(
    target_state: Res<TargetState>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    target_transforms: Query<&GlobalTransform, With<Targetable>>,
    controller_move: Single<&ControllerMove, With<Player>>,
    mut player: Single<
        (
            &mut Transform,
            &mut MoveTarget,
            &mut KnightAnimator,
            &mut PlayerCombat,
            &mut PlayerStats,
        ),
        With<Player>,
    >,
    mut attack_intent: Local<AttackIntent>,
    time: Res<Time>,
) {
    let Some(target_entity) = target_state.targeted else {
        attack_intent.clear();
        return;
    };
    let Ok(target_tf) = target_transforms.get(target_entity) else {
        attack_intent.clear();
        return;
    };

    let attack_pressed = mouse_buttons.just_pressed(MouseButton::Left)
        || any_gamepad_just_pressed(&gamepads, GamepadButton::RightTrigger);
    let attack_held = mouse_buttons.pressed(MouseButton::Left)
        || any_gamepad_pressed(&gamepads, GamepadButton::RightTrigger);

    // Click / bumper sets a pending strike that survives the walk; hold = continuous
    if attack_pressed {
        attack_intent.pending_strike = true;
        attack_intent.holding = true;
    }
    if !attack_held {
        attack_intent.holding = false;
    }

    let (ref mut player_tf, ref mut move_target, ref mut animator, ref mut combat, ref mut stats) =
        *player;
    let target_pos = target_tf.translation();
    let to_target = Vec3::new(
        target_pos.x - player_tf.translation.x,
        0.0,
        target_pos.z - player_tf.translation.z,
    );
    let distance = to_target.length();

    if distance > 0.001 {
        player_tf.look_to(-to_target.normalize(), Vec3::Y);
    }

    let can_attack = attack_intent.pending_strike || attack_intent.holding;

    if can_attack && distance > ATTACK_RANGE && !controller_move.active() {
        move_target.position = Some(clamp_ground_target_to_arena(
            Vec2::new(target_pos.x, target_pos.z),
            PLAYER_COLLISION_RADIUS,
        ));
        if distance > ATTACK_RANGE * 1.5 {
            attack_intent.windup = 0.0;
        }
    } else {
        move_target.position = None;
        if !can_attack {
            attack_intent.windup = 0.0;
        }
    }

    if can_attack && animator.swing_timer.finished() {
        attack_intent.windup += time.delta_secs();
        if attack_intent.windup >= ATTACK_WINDUP
            && distance <= ATTACK_RANGE
            && stats.spend_stamina(ATTACK_STAMINA_COST)
        {
            attack_intent.pending_strike = false;
            attack_intent.windup = 0.0;
            animator.swing_timer.reset();
            combat.swing_id = combat.swing_id.wrapping_add(1);
            combat.strike = 0.0;
        }
    } else if !can_attack {
        attack_intent.windup = 0.0;
    }
}

pub(super) fn trigger_dodge(
    keyboard: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut player: DodgePlayer<'_>,
    mut target_state: ResMut<TargetState>,
) {
    let (
        player_tf,
        ref mut dodge,
        ref mut move_target,
        ref mut stats,
        ref mut animator,
        ref mut combat,
    ) = *player;

    let dodge_pressed = keyboard.just_pressed(KeyCode::Space)
        || any_gamepad_just_pressed(&gamepads, GamepadButton::East);

    if !dodge_pressed || !dodge.cooldown.finished() || dodge.active {
        return;
    }

    if !stats.spend_stamina(DODGE_STAMINA_COST) {
        return;
    }

    let forward = visual_forward(player_tf).normalize_or_zero();
    let direction = if forward.length_squared() > 0.001 {
        Vec3::new(forward.x, 0.0, forward.z).normalize()
    } else {
        Vec3::Z
    };

    dodge.active = true;
    dodge.direction = direction;
    dodge.timer.reset();
    dodge.cooldown.reset();

    animator.cancel_swing();
    combat.strike = 0.0;
    move_target.position = None;
    target_state.targeted = None;
}

pub(super) fn update_dodge(
    time: Res<Time>,
    mut player: Single<(&mut Transform, &mut Dodge), With<Player>>,
) {
    let (ref mut player_tf, ref mut dodge) = *player;

    dodge.timer.tick(time.delta());
    dodge.cooldown.tick(time.delta());

    if !dodge.active {
        return;
    }

    if dodge.timer.finished() {
        dodge.active = false;
        return;
    }

    let step = DODGE_SPEED * time.delta_secs();
    player_tf.translation += dodge.direction * step;
    clamp_translation_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
}

pub(super) fn regen_stamina(time: Res<Time>, mut player: Single<&mut PlayerStats, With<Player>>) {
    player.regen_delay.tick(time.delta());
    if player.regen_delay.finished() && player.stamina < player.max_stamina {
        player.stamina =
            (player.stamina + super::STAMINA_REGEN * time.delta_secs()).min(player.max_stamina);
    }
}

pub(super) fn move_player_with_controller(
    mut player_query: Single<(&mut Transform, &mut MoveTarget, &ControllerMove, &Dodge), With<Player>>,
    time: Res<Time>,
) {
    let (ref mut player_tf, ref mut move_target, controller_move, dodge) = *player_query;

    if dodge.active {
        return;
    }

    let magnitude = controller_move.input.length().min(1.0);
    if magnitude <= 0.0 {
        return;
    }

    move_target.position = None;

    let direction = controller_move.input / magnitude;
    let facing = Vec3::new(direction.x, 0.0, direction.y);
    if facing.length_squared() > 0.0 {
        player_tf.look_to(-facing, Vec3::Y);
    }

    let step = MOVE_SPEED * magnitude * time.delta_secs();
    player_tf.translation.x += direction.x * step;
    player_tf.translation.z += direction.y * step;
    clamp_translation_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
}

pub(super) fn move_player(
    mut player_query: Single<(&mut Transform, &mut MoveTarget, &Dodge), With<Player>>,
    time: Res<Time>,
) {
    let (ref mut player_tf, ref mut move_target, dodge) = *player_query;

    if dodge.active {
        return;
    }

    let Some(target_position) = move_target.position else {
        return;
    };

    let current = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let diff = target_position - current;
    let distance = diff.length();

    if distance <= ARRIVE_THRESHOLD {
        move_target.position = None;
        return;
    }

    let direction = diff / distance;
    let facing = Vec3::new(direction.x, 0.0, direction.y);
    if facing.length_squared() > 0.0 {
        player_tf.look_to(-facing, Vec3::Y);
    }

    let step_mag = MOVE_SPEED * time.delta_secs();
    if step_mag >= distance {
        player_tf.translation.x = target_position.x;
        player_tf.translation.z = target_position.y;
        move_target.position = None;
    } else {
        player_tf.translation.x += direction.x * step_mag;
        player_tf.translation.z += direction.y * step_mag;
    }

    clamp_translation_to_arena(&mut player_tf.translation, PLAYER_COLLISION_RADIUS);
}

fn any_gamepad_just_pressed(gamepads: &Query<&Gamepad>, button: GamepadButton) -> bool {
    gamepads.iter().any(|gamepad| gamepad.just_pressed(button))
}

fn any_gamepad_pressed(gamepads: &Query<&Gamepad>, button: GamepadButton) -> bool {
    gamepads.iter().any(|gamepad| gamepad.pressed(button))
}

fn strongest_stick_direction(
    gamepads: &Query<&Gamepad>,
    stick_reader: impl Fn(&Gamepad) -> Vec2,
    camera_tf: &Transform,
    deadzone: f32,
) -> Vec2 {
    let mut best = Vec2::ZERO;
    let mut best_len = 0.0;

    for gamepad in gamepads.iter() {
        let world = camera_relative_input(stick_reader(gamepad), camera_tf, deadzone);
        let length = world.length_squared();
        if length > best_len {
            best = world;
            best_len = length;
        }
    }

    best
}

fn camera_relative_input(input: Vec2, camera_tf: &Transform, deadzone: f32) -> Vec2 {
    let magnitude = input.length().min(1.0);
    if magnitude <= deadzone {
        return Vec2::ZERO;
    }

    let right = horizontal_axis(camera_tf.rotation * Vec3::X);
    let forward = horizontal_axis(camera_tf.rotation * -Vec3::Z);
    let combined = right * input.x + forward * input.y;
    let combined_len = combined.length();
    if combined_len <= 0.0001 {
        return Vec2::ZERO;
    }

    let strength = ((magnitude - deadzone) / (1.0 - deadzone)).clamp(0.0, 1.0);
    let direction = combined / combined_len;
    Vec2::new(direction.x, direction.z) * strength
}

fn horizontal_axis(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z).normalize_or_zero()
}

fn find_directional_target(
    player_translation: Vec3,
    direction: Vec2,
    excluded: Option<Entity>,
    targetables: &Query<(Entity, &GlobalTransform), With<Targetable>>,
) -> Option<Entity> {
    let direction_len_sq = direction.length_squared();
    if direction_len_sq <= 0.0001 {
        return None;
    }

    let direction = direction / direction_len_sq.sqrt();
    let player_ground = Vec2::new(player_translation.x, player_translation.z);
    let mut best: Option<(Entity, f32)> = None;

    for (entity, transform) in targetables.iter() {
        if Some(entity) == excluded {
            continue;
        }

        let to_enemy = Vec2::new(
            transform.translation().x - player_ground.x,
            transform.translation().z - player_ground.y,
        );
        let distance_sq = to_enemy.length_squared();
        if distance_sq <= 0.0001 {
            continue;
        }

        let alignment = to_enemy.normalize().dot(direction);
        if alignment < TARGET_DIRECTION_DOT {
            continue;
        }

        match best {
            Some((_, best_distance_sq)) if distance_sq >= best_distance_sq => {}
            _ => best = Some((entity, distance_sq)),
        }
    }

    best.map(|(entity, _)| entity)
}

fn target_in_direction(player_translation: Vec3, direction: Vec2, target_translation: Vec3) -> bool {
    let direction_len_sq = direction.length_squared();
    if direction_len_sq <= 0.0001 {
        return false;
    }

    let direction = direction / direction_len_sq.sqrt();
    let to_enemy = Vec2::new(
        target_translation.x - player_translation.x,
        target_translation.z - player_translation.z,
    );
    if to_enemy.length_squared() <= 0.0001 {
        return false;
    }

    to_enemy.normalize().dot(direction) >= TARGET_DIRECTION_DOT
}
