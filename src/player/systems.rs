use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::MainCamera;
use crate::targeting::{TargetState, Targetable};
use crate::world::tilemap::{
    clamp_ground_target_to_arena, clamp_translation_to_arena,
};

use super::{
    visual_forward, Dodge, KnightAnimator, MoveTarget, Player, PlayerCombat, PlayerStats,
    ARRIVE_THRESHOLD, ATTACK_RANGE, ATTACK_STAMINA_COST, ATTACK_WINDUP, DODGE_SPEED,
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

pub(super) fn chase_and_attack_target(
    target_state: Res<TargetState>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    target_transforms: Query<&GlobalTransform, With<Targetable>>,
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

    // Click sets a pending strike that survives the walk; hold = continuous
    if mouse_buttons.just_pressed(MouseButton::Left) {
        attack_intent.pending_strike = true;
        attack_intent.holding = true;
    }
    if !mouse_buttons.pressed(MouseButton::Left) {
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

    // Face the target
    if distance > 0.001 {
        player_tf.look_to(-to_target.normalize(), Vec3::Y);
    }

    let can_attack = attack_intent.pending_strike || attack_intent.holding;

    if distance > ATTACK_RANGE {
        // Chase: move toward the target, keep winding up if close-ish
        move_target.position = Some(clamp_ground_target_to_arena(
            Vec2::new(target_pos.x, target_pos.z),
            PLAYER_COLLISION_RADIUS,
        ));
        if !can_attack || distance > ATTACK_RANGE * 1.5 {
            attack_intent.windup = 0.0;
        }
    } else {
        // In range: stop and plant feet
        move_target.position = None;
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

    if !keyboard.just_pressed(KeyCode::Space) || !dodge.cooldown.finished() || dodge.active {
        return;
    }

    if !stats.spend_stamina(DODGE_STAMINA_COST) {
        return;
    }

    // Dodge in current facing direction
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

    // Cancel current actions
    animator.cancel_swing();
    combat.strike = 0.0;
    move_target.position = None;
    target_state.targeted = None;
}

pub(super) fn update_dodge(time: Res<Time>, mut player: Single<(&mut Transform, &mut Dodge), With<Player>>) {
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

    // Move in dodge direction
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

pub(super) fn move_player(
    mut player_query: Single<(&mut Transform, &mut MoveTarget, &Dodge), With<Player>>,
    time: Res<Time>,
) {
    let (ref mut player_tf, ref mut move_target, dodge) = *player_query;

    // Don't move normally during dodge
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
        // The knight mesh is built facing +Z, but look_to aligns local -Z to the target.
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
