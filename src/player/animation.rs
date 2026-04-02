use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{smoothstep01, HitPoints};

use super::{
    dodge_motion_curve, AttackLunge, ControllerMove, DeathAnim, Dodge, JointRest, KnightAnimator,
    KnightJoint, MoveTarget, Player, PlayerCombat, DEATH_ANIM_DURATION,
};

type AnimatedPlayer<'w> = Single<
    'w,
    (
        &'static MoveTarget,
        &'static ControllerMove,
        &'static Dodge,
        &'static AttackLunge,
        &'static HitPoints,
        &'static mut KnightAnimator,
        &'static mut PlayerCombat,
    ),
    With<Player>,
>;

pub(super) fn animate_knight(
    time: Res<Time>,
    mut player: AnimatedPlayer<'_>,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let (move_target, controller_move, dodge, attack_lunge, hp, ref mut animator, ref mut combat) =
        *player;

    // Don't fight with animate_death over joint transforms
    if hp.is_dead() {
        return;
    }

    if dodge.active {
        animator.cancel_swing();
        combat.strike = 0.0;
    } else {
        animator.swing_timer.tick(time.delta());
    }

    // Lunge blend: peaks in the middle, ease-in-out
    let lunge_blend = if attack_lunge.active() {
        let p = attack_lunge.progress();
        (p * PI).sin()
    } else {
        0.0
    };

    let moving = move_target.position.is_some() || controller_move.active();

    if moving {
        animator.walk_phase += time.delta_secs() * 8.0;
        animator.walk_phase %= 100.0 * std::f32::consts::TAU;
    }

    let elapsed = time.elapsed_secs();
    let walk_swing = if moving {
        animator.walk_phase.sin()
    } else {
        0.0
    };
    let walk_bob = if moving {
        (animator.walk_phase * 2.0).sin() * 0.06
    } else {
        0.0
    };

    // Idle breathing / weight-shift (only when standing still and not swinging)
    let idle = !moving && !dodge.active && animator.swing_timer.finished();
    let breath = if idle { (elapsed * 1.4).sin() } else { 0.0 };
    let idle_shift = if idle { (elapsed * 0.6).sin() } else { 0.0 };
    let sword_swing = if dodge.active {
        0.0
    } else {
        sword_swing_curve(&animator.swing_timer)
    };
    let sword_windup = (-sword_swing).max(0.0);
    let sword_strike = sword_swing.max(0.0);
    combat.strike = sword_strike;

    // Dodge roll: 0.0 -> 1.0 over DODGE_DURATION
    let dodge_t = if dodge.active {
        let elapsed = dodge.timer.elapsed_secs();
        let duration = dodge.timer.duration().as_secs_f32().max(0.001);
        (elapsed / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Full forward tumble: smoothstep for natural acceleration/deceleration
    let roll_angle = if dodge.active {
        dodge_motion_curve(dodge_t) * PI * 2.0
    } else {
        0.0
    };
    // Drop hips low during the middle of the roll
    let roll_crouch = if dodge.active {
        (dodge_t * PI).sin() * 0.55
    } else {
        0.0
    };
    // Tuck limbs during roll
    let roll_tuck = if dodge.active {
        (dodge_t * PI).sin()
    } else {
        0.0
    };

    for (joint, rest, mut transform) in &mut joints {
        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        if dodge.active {
            match joint {
                KnightJoint::Hips => {
                    transform.translation.y -= roll_crouch;
                    transform.rotation *= Quat::from_rotation_x(roll_angle);
                }
                KnightJoint::Chest => {}
                KnightJoint::Head => {
                    transform.rotation *= Quat::from_rotation_x(-roll_tuck * 0.6);
                }
                KnightJoint::LeftArm => {
                    transform.rotation *= Quat::from_rotation_x(roll_tuck * 1.2)
                        * Quat::from_rotation_z(-roll_tuck * 0.3);
                }
                KnightJoint::RightArm => {
                    transform.rotation *= Quat::from_rotation_x(roll_tuck * 1.2)
                        * Quat::from_rotation_z(roll_tuck * 0.3);
                }
                KnightJoint::LeftLeg => {
                    transform.rotation *= Quat::from_rotation_x(-roll_tuck * 0.9);
                }
                KnightJoint::RightLeg => {
                    transform.rotation *= Quat::from_rotation_x(-roll_tuck * 0.9);
                }
                KnightJoint::Sword => {}
            }
            continue;
        }

        // Rotation budget (peak windup=0.50, peak strike=1.60):
        //   Arm X:   40° back  + 50° forward  = 90° arm arc
        //   Sword X: 17° back  + 29° forward  = 46° blade arc
        //   Sword Z:             37° cross-body sweep
        //   Body Y:   8° right + 31° left      = 39° torso twist
        //   Total visible blade sweep ≈ 130°
        match joint {
            KnightJoint::Hips => {
                transform.translation.y += walk_bob + breath * 0.008;
                transform.translation.z += sword_strike * 0.06 + lunge_blend * 0.10;
                transform.translation.y -= lunge_blend * 0.08;
                transform.rotation *= Quat::from_rotation_x(
                    -sword_strike * 0.08 - lunge_blend * 0.18,
                ) * Quat::from_rotation_y(
                    walk_swing * 0.05 - sword_windup * 0.10
                        + sword_strike * 0.14
                        + idle_shift * 0.018,
                ) * Quat::from_rotation_z(idle_shift * 0.012);
            }
            KnightJoint::Chest => {
                transform.translation.y += walk_bob * 0.4 + breath * 0.014;
                transform.rotation *= Quat::from_rotation_x(
                    sword_windup * 0.08 - sword_strike * 0.14 + breath * 0.018
                        - lunge_blend * 0.12,
                ) * Quat::from_rotation_y(
                    -walk_swing * 0.12 - sword_windup * 0.18 + sword_strike * 0.20
                        - idle_shift * 0.012,
                ) * Quat::from_rotation_z(-sword_windup * 0.03 + sword_strike * 0.05);
            }
            KnightJoint::Head => {
                transform.rotation *= Quat::from_rotation_x(
                    sword_windup * 0.03 - sword_strike * 0.05 - breath * 0.010 - walk_bob * 0.15,
                ) * Quat::from_rotation_y(
                    sword_windup * 0.04 - sword_strike * 0.06
                        + idle_shift * 0.035
                        + walk_swing * 0.04,
                );
            }
            KnightJoint::LeftArm => {
                // Shield arm: small sympathetic brace, mostly stays put
                transform.rotation *= Quat::from_rotation_x(
                    -walk_swing * 0.50 + sword_windup * 0.06 - sword_strike * 0.08 + breath * 0.02,
                ) * Quat::from_rotation_z(-idle_shift * 0.015);
            }
            KnightJoint::RightArm => {
                // Sword arm: ~40° pullback, ~50° forward sweep
                transform.rotation *= Quat::from_rotation_x(
                    walk_swing * 0.18 + sword_windup * 1.40 - sword_strike * 0.90 - breath * 0.015,
                ) * Quat::from_rotation_y(
                    sword_windup * 0.15 - sword_strike * 0.10,
                ) * Quat::from_rotation_z(
                    sword_windup * 0.06 - sword_strike * 0.10 + idle_shift * 0.012,
                );
            }
            KnightJoint::LeftLeg => {
                transform.rotation *= Quat::from_rotation_x(
                    walk_swing * 0.60 + sword_strike * 0.08 - lunge_blend * 0.30,
                );
            }
            KnightJoint::RightLeg => {
                transform.rotation *= Quat::from_rotation_x(
                    -walk_swing * 0.60 - sword_strike * 0.06 + lunge_blend * 0.20,
                );
            }
            KnightJoint::Sword => {
                // Blade raises on windup, sweeps down and across on strike
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 0.60 - sword_strike * 0.50)
                        * Quat::from_rotation_z(-sword_strike * 0.40);
            }
        }
    }
}

// Sword swing phase boundaries (fraction of total swing duration)
const SWING_WINDUP_END: f32 = 0.16;
const SWING_STRIKE_END: f32 = 0.42;
// Amplitude at each phase transition
const SWING_WINDUP_PEAK: f32 = -0.50;
const SWING_STRIKE_TRAVEL: f32 = 2.10;
const SWING_STRIKE_PEAK: f32 = SWING_WINDUP_PEAK + SWING_STRIKE_TRAVEL;

fn sword_swing_curve(timer: &Timer) -> f32 {
    if timer.finished() {
        return 0.0;
    }

    let duration = timer.duration().as_secs_f32();
    if duration <= 0.0 {
        return 0.0;
    }

    let t = timer.elapsed_secs() / duration;
    if t < SWING_WINDUP_END {
        // Smooth anticipation pullback
        SWING_WINDUP_PEAK * smoothstep01(t / SWING_WINDUP_END)
    } else if t < SWING_STRIKE_END {
        let strike_t = (t - SWING_WINDUP_END) / (SWING_STRIKE_END - SWING_WINDUP_END);
        // Cubic ease-out: explosive start, gradual deceleration
        let ease = 1.0 - (1.0 - strike_t).powi(3);
        SWING_WINDUP_PEAK + SWING_STRIKE_TRAVEL * ease
    } else {
        let recover_t = (t - SWING_STRIKE_END) / (1.0 - SWING_STRIKE_END);
        SWING_STRIKE_PEAK * (1.0 - smoothstep01(recover_t))
    }
}

pub(super) fn animate_death(
    time: Res<Time>,
    mut player: Query<(&mut DeathAnim, &HitPoints), With<Player>>,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let Ok((mut death_anim, hp)) = player.get_single_mut() else {
        return;
    };

    if !hp.is_dead() {
        return;
    }

    if !death_anim.active {
        death_anim.active = true;
        death_anim.timer.reset();
    }

    death_anim.timer.tick(time.delta());

    let t = (death_anim.timer.elapsed_secs() / DEATH_ANIM_DURATION).clamp(0.0, 1.0);
    let fall = smoothstep01(t);

    // Fall sideways to the ground
    let fall_rotation = fall * (PI / 2.0); // 90 degrees to the side
    let fall_drop = fall * 0.7; // hips drop toward ground

    for (joint, rest, mut transform) in &mut joints {
        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        match joint {
            KnightJoint::Hips => {
                transform.translation.y -= fall_drop;
                transform.rotation *= Quat::from_rotation_z(fall_rotation);
            }
            KnightJoint::Head => {
                // Head lolls forward
                transform.rotation *= Quat::from_rotation_x(fall * 0.4);
            }
            KnightJoint::LeftArm => {
                transform.rotation *=
                    Quat::from_rotation_x(-fall * 0.6) * Quat::from_rotation_z(-fall * 0.3);
            }
            KnightJoint::RightArm => {
                transform.rotation *=
                    Quat::from_rotation_x(-fall * 0.8) * Quat::from_rotation_z(fall * 0.4);
            }
            KnightJoint::Sword => {
                // Sword drops away
                transform.rotation *=
                    Quat::from_rotation_x(-fall * 1.2) * Quat::from_rotation_z(fall * 0.6);
            }
            _ => {}
        }
    }
}
