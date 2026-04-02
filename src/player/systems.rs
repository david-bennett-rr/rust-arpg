use bevy::ecs::system::SystemParam;
use bevy::window::PrimaryWindow;
use bevy::{input::gamepad::Gamepad, prelude::*};

use crate::camera::MainCamera;
use crate::targeting::{TargetState, Targetable};
use crate::world::tilemap::{
    clamp_ground_target, segment_clear_of_walls, sweep_ground_target, FloorBounds, Wall,
    WallCollider,
};

use super::{
    dodge_motion_curve, visual_forward, ControllerMove, Dodge, KnightAnimator, MoveTarget, Player,
    PlayerCombat, PlayerStats, ARRIVE_THRESHOLD, ATTACK_RANGE, ATTACK_STAMINA_COST, ATTACK_WINDUP,
    DODGE_DISTANCE, DODGE_STAMINA_COST, MOVE_SPEED, PLAYER_COLLISION_RADIUS,
};

type DodgePlayer<'w> = Single<
    'w,
    (
        &'static Transform,
        &'static mut Dodge,
        &'static mut MoveTarget,
        &'static ControllerMove,
        &'static mut PlayerStats,
        &'static mut KnightAnimator,
        &'static mut PlayerCombat,
    ),
    With<Player>,
>;

type AttackPlayer<'w> = Single<
    'w,
    (
        &'static mut Transform,
        &'static mut MoveTarget,
        &'static ControllerMove,
        &'static mut KnightAnimator,
        &'static mut PlayerCombat,
        &'static mut PlayerStats,
    ),
    With<Player>,
>;

type PlayerWallQuery<'w, 's> =
    Query<'w, 's, (&'static Transform, &'static WallCollider), (With<Wall>, Without<Player>)>;

type DirectionalTargetables<'w, 's> =
    Query<'w, 's, (Entity, &'static GlobalTransform, &'static Visibility), With<Targetable>>;

const MOVE_DEADZONE: f32 = 0.18;
const TARGET_SWITCH_DEADZONE: f32 = 0.72;
const TARGET_DIRECTION_DOT: f32 = 0.2;
const MAX_TARGET_DISTANCE: f32 = 20.0;
const PLAYER_WALL_CLEARANCE: f32 = 0.08;

#[derive(Debug, Default)]
pub(super) struct AttackIntent {
    pending_strike: bool,
    holding: bool,
    windup: f32,
}

#[derive(SystemParam)]
pub(super) struct AttackControls<'w, 's> {
    target_state: Res<'w, TargetState>,
    mouse_buttons: Res<'w, ButtonInput<MouseButton>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
    time: Res<'w, Time>,
    bounds: Res<'w, FloorBounds>,
}

#[derive(SystemParam)]
pub(super) struct ClickContext<'w, 's> {
    window: Option<Single<'w, &'static Window, With<PrimaryWindow>>>,
    camera_query: Option<Single<'w, (&'static Camera, &'static GlobalTransform), With<MainCamera>>>,
    player_tf: Single<'w, &'static Transform, With<Player>>,
    bounds: Res<'w, FloorBounds>,
    walls: PlayerWallQuery<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct ControllerTargetingContext<'w, 's> {
    target_state: ResMut<'w, TargetState>,
    targetables: DirectionalTargetables<'w, 's>,
    walls: PlayerWallQuery<'w, 's>,
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

    player.input = strongest_stick_direction(
        &gamepads,
        |gamepad| gamepad.left_stick(),
        *camera_tf,
        MOVE_DEADZONE,
    );
}

pub(super) fn handle_left_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    ctx: ClickContext<'_, '_>,
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

    let Some(window) = ctx.window else {
        return;
    };
    let Some(camera_query) = ctx.camera_query else {
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
    let player_ground = Vec2::new(ctx.player_tf.translation.x, ctx.player_tf.translation.z);
    let swept = sweep_ground_target(
        &ctx.bounds,
        player_ground,
        Vec2::new(point.x, point.z),
        PLAYER_COLLISION_RADIUS,
        ctx.walls.iter(),
        PLAYER_WALL_CLEARANCE,
    );
    player.position = Some(swept);
}

pub(super) fn handle_controller_targeting(
    gamepads: Query<&Gamepad>,
    camera_query: Option<Single<&Transform, With<MainCamera>>>,
    controller_move: Single<&ControllerMove, With<Player>>,
    player: Single<&Transform, With<Player>>,
    mut ctx: ControllerTargetingContext<'_, '_>,
    mut right_stick_released: Local<bool>,
) {
    if let Some(targeted) = ctx.target_state.targeted {
        if ctx
            .targetables
            .get(targeted)
            .ok()
            .is_none_or(|(_, _, visibility)| *visibility == Visibility::Hidden)
        {
            ctx.target_state.targeted = None;
        }
    }

    if let Some(camera_tf) = camera_query {
        let right_input = strongest_stick_direction(
            &gamepads,
            |gamepad| gamepad.right_stick(),
            *camera_tf,
            MOVE_DEADZONE,
        );
        let right_active =
            right_input.length_squared() >= TARGET_SWITCH_DEADZONE * TARGET_SWITCH_DEADZONE;

        if right_active && *right_stick_released {
            let excluded = ctx.target_state.targeted;
            if let Some(target) = find_directional_target(
                player.translation,
                right_input,
                excluded,
                &ctx.targetables,
                &ctx.walls,
            )
            .or_else(|| {
                find_directional_target(
                    player.translation,
                    right_input,
                    None,
                    &ctx.targetables,
                    &ctx.walls,
                )
            }) {
                ctx.target_state.targeted = Some(target);
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
        let keep_current = ctx
            .target_state
            .targeted
            .and_then(|entity| ctx.targetables.get(entity).ok())
            .is_some_and(
                |(_, transform, visibility): (Entity, &GlobalTransform, &Visibility)| {
                    *visibility != Visibility::Hidden
                        && target_in_direction(
                            player.translation,
                            direction,
                            transform.translation(),
                        )
                        && target_visible_from_player(
                            player.translation,
                            transform.translation(),
                            &ctx.walls,
                        )
                },
            );

        if !keep_current {
            ctx.target_state.targeted = find_directional_target(
                player.translation,
                direction,
                None,
                &ctx.targetables,
                &ctx.walls,
            );
        }
    }
}

pub(super) fn chase_and_attack_target(
    controls: AttackControls<'_, '_>,
    target_transforms: Query<(&GlobalTransform, &Visibility), With<Targetable>>,
    walls: PlayerWallQuery<'_, '_>,
    mut player: AttackPlayer<'_>,
    mut attack_intent: Local<AttackIntent>,
) {
    let (attack_pressed, attack_held) = resolve_attack_input(
        controls.target_state.targeted.is_some(),
        controls.mouse_buttons.just_pressed(MouseButton::Left),
        controls.mouse_buttons.pressed(MouseButton::Left),
        any_gamepad_just_pressed(&controls.gamepads, GamepadButton::RightTrigger),
        any_gamepad_pressed(&controls.gamepads, GamepadButton::RightTrigger),
    );

    if attack_pressed {
        attack_intent.pending_strike = true;
        attack_intent.holding = true;
    }
    if !attack_held {
        attack_intent.holding = false;
    }

    let (
        ref mut player_tf,
        ref mut move_target,
        controller_move,
        ref mut animator,
        ref mut combat,
        ref mut stats,
    ) = *player;

    // Resolve target: face it, chase it if needed
    let has_target = if let Some(target_entity) = controls.target_state.targeted {
        if let Ok((target_tf, visibility)) = target_transforms.get(target_entity) {
            if *visibility == Visibility::Hidden {
                move_target.position = None;
                attack_intent.windup = 0.0;
                false
            } else {
                let target_pos = target_tf.translation();
                let to_target = Vec3::new(
                    target_pos.x - player_tf.translation.x,
                    0.0,
                    target_pos.z - player_tf.translation.z,
                );
                let distance = to_target.length();
                let clear_path = segment_clear_of_walls(
                    Vec2::new(player_tf.translation.x, player_tf.translation.z),
                    Vec2::new(target_pos.x, target_pos.z),
                    PLAYER_COLLISION_RADIUS,
                    walls.iter(),
                );

                if distance > 0.001 {
                    player_tf.look_to(-to_target.normalize(), Vec3::Y);
                }

                let can_attack = attack_intent.pending_strike || attack_intent.holding;

                // Chase toward target if out of range
                if can_attack && clear_path && distance > ATTACK_RANGE && !controller_move.active()
                {
                    move_target.position = Some(clamp_ground_target(
                        &controls.bounds,
                        Vec2::new(target_pos.x, target_pos.z),
                        PLAYER_COLLISION_RADIUS,
                    ));
                    if distance > ATTACK_RANGE * 1.5 {
                        attack_intent.windup = 0.0;
                    }
                } else {
                    move_target.position = None;
                    if !can_attack || !clear_path {
                        attack_intent.windup = 0.0;
                    }
                }

                clear_path && distance <= ATTACK_RANGE
            }
        } else {
            true // target entity exists but transform gone — allow swing anyway
        }
    } else {
        true // no target — always allow swing in facing direction
    };

    let can_attack = attack_intent.pending_strike || attack_intent.holding;

    if advance_attack_windup(
        &mut attack_intent,
        can_attack && has_target,
        animator.swing_timer.finished(),
        controls.time.delta_secs(),
    ) && stats.spend_stamina(ATTACK_STAMINA_COST)
    {
        animator.swing_timer.reset();
        combat.swing_id = combat.swing_id.wrapping_add(1);
        combat.strike = 0.0;
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
        controller_move,
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

    let direction = dodge_direction(player_tf, controller_move, move_target);

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
    bounds: Res<FloorBounds>,
    walls: PlayerWallQuery<'_, '_>,
    mut player: Single<(&mut Transform, &mut Dodge), With<Player>>,
) {
    let (ref mut player_tf, ref mut dodge) = *player;

    if !dodge.active {
        dodge.cooldown.tick(time.delta());
        return;
    }

    let previous_progress = dodge_progress(dodge);
    dodge.timer.tick(time.delta());
    dodge.cooldown.tick(time.delta());

    let progress = dodge_progress(dodge);
    let step = DODGE_DISTANCE
        * (dodge_motion_curve(progress) - dodge_motion_curve(previous_progress)).max(0.0);
    let current = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let desired_end = current + Vec2::new(dodge.direction.x, dodge.direction.z) * step;
    let swept = sweep_ground_target(
        &bounds,
        current,
        desired_end,
        PLAYER_COLLISION_RADIUS,
        walls.iter(),
        PLAYER_WALL_CLEARANCE,
    );
    player_tf.translation.x = swept.x;
    player_tf.translation.z = swept.y;

    if progress >= 1.0 {
        dodge.active = false;
    }
}

fn dodge_progress(dodge: &Dodge) -> f32 {
    let duration = dodge.timer.duration().as_secs_f32().max(0.001);
    (dodge.timer.elapsed_secs() / duration).clamp(0.0, 1.0)
}

fn dodge_direction(
    player_tf: &Transform,
    controller_move: &ControllerMove,
    move_target: &MoveTarget,
) -> Vec3 {
    if controller_move.active() {
        return Vec3::new(controller_move.input.x, 0.0, controller_move.input.y).normalize();
    }

    if let Some(target) = move_target.position {
        let to_target = target - Vec2::new(player_tf.translation.x, player_tf.translation.z);
        if to_target.length_squared() > 0.0001 {
            return Vec3::new(to_target.x, 0.0, to_target.y).normalize();
        }
    }

    let forward = visual_forward(player_tf).normalize_or_zero();
    if forward.length_squared() > 0.001 {
        Vec3::new(forward.x, 0.0, forward.z).normalize()
    } else {
        Vec3::Z
    }
}

pub(super) fn regen_stamina(time: Res<Time>, mut player: Single<&mut PlayerStats, With<Player>>) {
    player.regen_delay.tick(time.delta());
    if player.regen_delay.finished() && player.stamina < player.max_stamina {
        player.stamina =
            (player.stamina + super::STAMINA_REGEN * time.delta_secs()).min(player.max_stamina);
    }
}

pub(super) fn move_player_with_controller(
    mut player_query: Single<
        (&mut Transform, &mut MoveTarget, &ControllerMove, &Dodge),
        With<Player>,
    >,
    time: Res<Time>,
    bounds: Res<FloorBounds>,
    walls: PlayerWallQuery<'_, '_>,
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
    let current = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let desired_end = current + direction * step;
    let swept = sweep_ground_target(
        &bounds,
        current,
        desired_end,
        PLAYER_COLLISION_RADIUS,
        walls.iter(),
        PLAYER_WALL_CLEARANCE,
    );
    player_tf.translation.x = swept.x;
    player_tf.translation.z = swept.y;
}

pub(super) fn move_player(
    mut player_query: Single<(&mut Transform, &mut MoveTarget, &Dodge), With<Player>>,
    time: Res<Time>,
    bounds: Res<FloorBounds>,
    walls: PlayerWallQuery<'_, '_>,
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
    let current = Vec2::new(player_tf.translation.x, player_tf.translation.z);
    let desired_end = if step_mag >= distance {
        target_position
    } else {
        current + direction * step_mag
    };
    let swept = sweep_ground_target(
        &bounds,
        current,
        desired_end,
        PLAYER_COLLISION_RADIUS,
        walls.iter(),
        PLAYER_WALL_CLEARANCE,
    );
    player_tf.translation.x = swept.x;
    player_tf.translation.z = swept.y;

    if step_mag >= distance && swept.distance_squared(target_position) <= 0.0001 {
        move_target.position = None;
    }
}

fn any_gamepad_just_pressed(gamepads: &Query<&Gamepad>, button: GamepadButton) -> bool {
    gamepads.iter().any(|gamepad| gamepad.just_pressed(button))
}

fn any_gamepad_pressed(gamepads: &Query<&Gamepad>, button: GamepadButton) -> bool {
    gamepads.iter().any(|gamepad| gamepad.pressed(button))
}

fn resolve_attack_input(
    mouse_target_locked: bool,
    mouse_pressed: bool,
    mouse_held: bool,
    gamepad_pressed: bool,
    gamepad_held: bool,
) -> (bool, bool) {
    let mouse_attack_pressed = mouse_target_locked && mouse_pressed;
    let mouse_attack_held = mouse_target_locked && mouse_held;
    (
        mouse_attack_pressed || gamepad_pressed,
        mouse_attack_held || gamepad_held,
    )
}

fn advance_attack_windup(
    attack_intent: &mut AttackIntent,
    can_attack: bool,
    swing_ready: bool,
    delta_secs: f32,
) -> bool {
    if can_attack && swing_ready {
        attack_intent.windup += delta_secs;
        if attack_intent.windup >= ATTACK_WINDUP {
            attack_intent.pending_strike = false;
            attack_intent.windup = 0.0;
            return true;
        }
    } else if !can_attack {
        attack_intent.windup = 0.0;
    }

    false
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
    targetables: &DirectionalTargetables<'_, '_>,
    walls: &PlayerWallQuery<'_, '_>,
) -> Option<Entity> {
    let direction_len_sq = direction.length_squared();
    if direction_len_sq <= 0.0001 {
        return None;
    }

    let direction = direction / direction_len_sq.sqrt();
    let player_ground = Vec2::new(player_translation.x, player_translation.z);
    let mut best: Option<(Entity, f32)> = None;

    for (entity, transform, visibility) in targetables.iter() {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let transform: &GlobalTransform = transform;
        if Some(entity) == excluded {
            continue;
        }

        let to_enemy = Vec2::new(
            transform.translation().x - player_ground.x,
            transform.translation().z - player_ground.y,
        );
        let distance_sq = to_enemy.length_squared();
        if distance_sq <= 0.0001 || distance_sq > MAX_TARGET_DISTANCE * MAX_TARGET_DISTANCE {
            continue;
        }
        if !target_visible_from_player(player_translation, transform.translation(), walls) {
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

fn target_visible_from_player(
    player_translation: Vec3,
    target_translation: Vec3,
    walls: &PlayerWallQuery<'_, '_>,
) -> bool {
    segment_clear_of_walls(
        Vec2::new(player_translation.x, player_translation.z),
        Vec2::new(target_translation.x, target_translation.z),
        0.0,
        walls.iter(),
    )
}

fn target_in_direction(
    player_translation: Vec3,
    direction: Vec2,
    target_translation: Vec3,
) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_click_does_not_arm_mouse_attack_without_target() {
        assert_eq!(
            resolve_attack_input(false, true, true, false, false),
            (false, false)
        );
    }

    #[test]
    fn targeted_click_arms_mouse_attack() {
        assert_eq!(
            resolve_attack_input(true, true, true, false, false),
            (true, true)
        );
    }

    #[test]
    fn failed_single_click_is_consumed_after_windup() {
        let mut intent = AttackIntent {
            pending_strike: true,
            holding: false,
            windup: ATTACK_WINDUP - 0.01,
        };

        assert!(advance_attack_windup(&mut intent, true, true, 0.02));
        assert!(!intent.pending_strike);
        assert!(!intent.holding);
        assert_eq!(intent.windup, 0.0);
        assert!(!advance_attack_windup(&mut intent, false, true, 0.02));
    }
}
