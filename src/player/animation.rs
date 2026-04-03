use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{smoothstep01, HitPoints};
use crate::hud::{RestPose, RestTransitionState};

use super::{
    dodge_motion_curve, AttackKind, AttackLunge, ControllerMove, DeathAnim, Dodge, JointRest,
    KnightAnimator, KnightJoint, MoveTarget, Player, PlayerCombat, RunState, DEATH_ANIM_DURATION,
};

type AnimatedPlayer<'w> = Single<
    'w,
    (
        &'static MoveTarget,
        &'static ControllerMove,
        &'static Dodge,
        &'static RunState,
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
    let (
        move_target,
        controller_move,
        dodge,
        run_state,
        attack_lunge,
        hp,
        ref mut animator,
        ref mut combat,
    ) = *player;

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
    let run_blend = if run_state.active && moving { 1.0 } else { 0.0 };
    let stride_scale = 1.0 + run_blend * 0.45;

    if moving {
        animator.walk_phase += time.delta_secs() * (8.0 + run_blend * 4.0);
        animator.walk_phase %= 100.0 * std::f32::consts::TAU;
    }

    let elapsed = time.elapsed_secs();
    let walk_swing = if moving {
        animator.walk_phase.sin()
    } else {
        0.0
    };
    let walk_bob = if moving {
        (animator.walk_phase * 2.0).sin() * (0.06 + run_blend * 0.03)
    } else {
        0.0
    };

    // Idle breathing / weight-shift (only when standing still and not swinging)
    let idle = !moving && !dodge.active && animator.swing_timer.finished();
    let breath = if idle { (elapsed * 1.4).sin() } else { 0.0 };
    let idle_shift = if idle { (elapsed * 0.6).sin() } else { 0.0 };
    let is_heavy = combat.attack_kind == AttackKind::Heavy;
    let (sword_windup, sword_strike, thrust) = if dodge.active {
        (0.0, 0.0, 0.0)
    } else if is_heavy && !animator.swing_timer.finished() {
        let t = thrust_curve(&animator.swing_timer);
        let windup = (-t).max(0.0);
        let strike = t.max(0.0);
        (0.0, 0.0, strike - windup)
    } else {
        let s = sword_swing_curve(&animator.swing_timer);
        ((-s).max(0.0), s.max(0.0), 0.0)
    };
    combat.strike = sword_strike.max(thrust.max(0.0));

    // Dodge roll: 0.0 -> 1.0 over DODGE_DURATION
    let dodge_t = if dodge.active {
        let elapsed = dodge.timer.elapsed_secs();
        let duration = dodge.timer.duration().as_secs_f32().max(0.001);
        (elapsed / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Full forward tumble: pronounced ease-out to match the longer dodge settle
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

        // Thrust decomposition: negative = pull back, positive = stab forward
        let thrust_windup = (-thrust).max(0.0);
        let thrust_strike = thrust.max(0.0);

        match joint {
            KnightJoint::Hips => {
                transform.translation.y += walk_bob + breath * 0.008;
                transform.translation.z +=
                    sword_strike * 0.08 + thrust_strike * 0.16 + lunge_blend * 0.10;
                transform.translation.y -= sword_strike * 0.05
                    + thrust_strike * 0.06
                    + lunge_blend * 0.08
                    + run_blend * 0.06;
                transform.rotation *= Quat::from_rotation_x(
                    -sword_windup * 0.06 + sword_strike * 0.14 - thrust_windup * 0.08
                        + thrust_strike * 0.20
                        - lunge_blend * 0.18
                        - run_blend * 0.22,
                ) * Quat::from_rotation_y(
                    walk_swing * 0.05 - sword_windup * 0.06
                        + sword_strike * 0.08
                        + idle_shift * 0.018,
                ) * Quat::from_rotation_z(idle_shift * 0.012);
            }
            KnightJoint::Chest => {
                transform.translation.y += walk_bob * 0.4 + breath * 0.014;
                transform.rotation *= Quat::from_rotation_x(
                    sword_windup * 0.12 - sword_strike * 0.18 + breath * 0.018
                        - lunge_blend * 0.12
                        + thrust_windup * 0.14
                        - thrust_strike * 0.24
                        - run_blend * 0.18,
                ) * Quat::from_rotation_y(
                    -walk_swing * 0.12 - sword_windup * 0.10 + sword_strike * 0.12
                        - idle_shift * 0.012
                        - thrust_windup * 0.08,
                ) * Quat::from_rotation_z(
                    sword_windup * 0.03 - sword_strike * 0.05 - thrust_strike * 0.04,
                );
            }
            KnightJoint::Head => {
                transform.rotation *= Quat::from_rotation_x(
                    sword_windup * 0.04 - sword_strike * 0.06
                        - breath * 0.010
                        - walk_bob * 0.15
                        + thrust_windup * 0.05
                        - thrust_strike * 0.08
                        + run_blend * 0.10,
                ) * Quat::from_rotation_y(
                    -sword_windup * 0.03
                        + sword_strike * 0.04
                        + idle_shift * 0.035
                        + walk_swing * 0.04,
                );
            }
            KnightJoint::LeftArm => {
                // Shield arm: brace harder during thrust
                transform.rotation *= Quat::from_rotation_x(
                    -walk_swing * 0.50 + sword_windup * 0.06 - sword_strike * 0.08 + breath * 0.02
                        - thrust_strike * 0.30
                        - walk_swing * run_blend * 0.25,
                ) * Quat::from_rotation_z(
                    -idle_shift * 0.015 - thrust_strike * 0.12 - run_blend * 0.08,
                );
            }
            KnightJoint::RightArm => {
                // Light: overhead downward slash. Heavy: heavy overhead chop.
                // Z rotation raises the arm overhead; X rotation swings it forward/down.
                transform.rotation *= Quat::from_rotation_x(
                    walk_swing * 0.18 + sword_windup * 0.35 - sword_strike * 1.30
                        - breath * 0.015
                        + thrust_windup * 0.45
                        - thrust_strike * 1.60
                        + walk_swing * run_blend * 0.50,
                ) * Quat::from_rotation_y(
                    -sword_windup * 0.06 + sword_strike * 0.04,
                ) * Quat::from_rotation_z(
                    -sword_windup * 0.90 + sword_strike * 0.35 + idle_shift * 0.012
                        - thrust_windup * 1.10
                        + thrust_strike * 0.40
                        + run_blend * 0.10,
                );
            }
            KnightJoint::LeftLeg => {
                // Front leg braces during thrust
                transform.rotation *= Quat::from_rotation_x(
                    walk_swing * 0.60 * stride_scale + sword_strike * 0.08
                        - lunge_blend * 0.30
                        - thrust_strike * 0.25,
                );
            }
            KnightJoint::RightLeg => {
                // Back leg pushes during thrust
                transform.rotation *= Quat::from_rotation_x(
                    -walk_swing * 0.60 * stride_scale - sword_strike * 0.06
                        + lunge_blend * 0.20
                        + thrust_strike * 0.20,
                );
            }
            KnightJoint::Sword => {
                // Light: downward slash. Heavy: overhead chop.
                // Windup tilts blade back (overhead); strike drives it forward/down.
                transform.rotation *=
                    Quat::from_rotation_x(
                        sword_windup * 0.40 - sword_strike * 0.60 + thrust_windup * 0.45
                            - thrust_strike * 0.80,
                    ) * Quat::from_rotation_z(
                        -sword_windup * 0.30 + sword_strike * 0.20
                            - thrust_windup * 0.35 + thrust_strike * 0.25,
                    );
            }
        }
    }
}

pub(super) fn animate_rest(
    time: Res<Time>,
    rest_transition: Option<Res<RestTransitionState>>,
    player: Query<&HitPoints, With<Player>>,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let Some(rest_transition) = rest_transition else {
        return;
    };
    let pose = rest_transition.pose();
    if pose == RestPose::Inactive {
        return;
    }

    let Ok(hp) = player.get_single() else {
        return;
    };
    if hp.is_dead() {
        return;
    }

    let settle = match pose {
        RestPose::Inactive => 0.0,
        RestPose::SitDown(progress) => smoothstep01(progress),
        RestPose::Seated => 1.0,
        RestPose::StandUp(progress) => 1.0 - smoothstep01(progress),
    };
    let seated_idle = if pose == RestPose::Seated {
        (time.elapsed_secs() * 1.8).sin()
    } else {
        0.0
    };
    let fire_warm = if pose == RestPose::Seated {
        (time.elapsed_secs() * 2.4).sin()
    } else {
        0.0
    };

    for (joint, rest, mut transform) in &mut joints {
        transform.translation = rest.translation;
        transform.rotation = rest.rotation;

        match joint {
            // A small half-kneel by the fire instead of pitching flat onto the ground.
            KnightJoint::Hips => {
                transform.translation.y -= 0.34 * settle;
                transform.translation.z -= 0.08 * settle;
                transform.rotation *= Quat::from_rotation_x(0.16 * settle)
                    * Quat::from_rotation_y(fire_warm * 0.03)
                    * Quat::from_rotation_z(-0.06 * settle);
            }
            KnightJoint::Chest => {
                transform.translation.y += 0.03 * settle;
                transform.translation.z -= 0.02 * settle;
                transform.rotation *= Quat::from_rotation_x(-0.22 * settle + seated_idle * 0.03)
                    * Quat::from_rotation_y(fire_warm * 0.05)
                    * Quat::from_rotation_z(0.05 * settle);
            }
            KnightJoint::Head => {
                transform.rotation *= Quat::from_rotation_x(0.08 * settle - seated_idle * 0.04)
                    * Quat::from_rotation_y(fire_warm * 0.05);
            }
            KnightJoint::LeftArm => {
                transform.rotation *= Quat::from_rotation_x(0.56 * settle + seated_idle * 0.04)
                    * Quat::from_rotation_y(-0.16 * settle)
                    * Quat::from_rotation_z(-0.78 * settle - fire_warm * 0.03);
            }
            KnightJoint::RightArm => {
                transform.rotation *= Quat::from_rotation_x(0.38 * settle - seated_idle * 0.03)
                    * Quat::from_rotation_y(0.08 * settle)
                    * Quat::from_rotation_z(0.42 * settle + fire_warm * 0.03);
            }
            KnightJoint::LeftLeg => {
                transform.rotation *= Quat::from_rotation_x(-0.92 * settle)
                    * Quat::from_rotation_y(-0.14 * settle)
                    * Quat::from_rotation_z(-0.52 * settle);
            }
            KnightJoint::RightLeg => {
                transform.rotation *= Quat::from_rotation_x(-1.08 * settle)
                    * Quat::from_rotation_y(0.08 * settle)
                    * Quat::from_rotation_z(0.16 * settle);
            }
            KnightJoint::Sword => {
                transform.rotation *=
                    Quat::from_rotation_x(-1.18 * settle) * Quat::from_rotation_z(0.76 * settle);
            }
        }
    }
}

// Sword swing phase boundaries (fraction of total swing duration)
const SWING_WINDUP_END: f32 = 0.40;
const SWING_STRIKE_END: f32 = 0.56;
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
        // Slow, deliberate raise — ease-out so it decelerates at the top
        SWING_WINDUP_PEAK * smoothstep01(t / SWING_WINDUP_END)
    } else if t < SWING_STRIKE_END {
        let strike_t = (t - SWING_WINDUP_END) / (SWING_STRIKE_END - SWING_WINDUP_END);
        // Cubic ease-in: accelerates into impact like a heavy downward blow
        let ease = strike_t * strike_t * strike_t;
        SWING_WINDUP_PEAK + SWING_STRIKE_TRAVEL * ease
    } else {
        let recover_t = (t - SWING_STRIKE_END) / (1.0 - SWING_STRIKE_END);
        SWING_STRIKE_PEAK * (1.0 - smoothstep01(recover_t))
    }
}

// Thrust phase boundaries (fraction of total swing duration)
const THRUST_WINDUP_END: f32 = 0.48;
const THRUST_STRIKE_END: f32 = 0.62;
// Amplitude: negative = raise up, positive = chop down
const THRUST_WINDUP_PEAK: f32 = -0.40;
const THRUST_STRIKE_TRAVEL: f32 = 1.80;
const THRUST_STRIKE_PEAK: f32 = THRUST_WINDUP_PEAK + THRUST_STRIKE_TRAVEL;

fn thrust_curve(timer: &Timer) -> f32 {
    if timer.finished() {
        return 0.0;
    }

    let duration = timer.duration().as_secs_f32();
    if duration <= 0.0 {
        return 0.0;
    }

    let t = timer.elapsed_secs() / duration;
    if t < THRUST_WINDUP_END {
        // Slow, heavy raise — deliberate and weighty
        THRUST_WINDUP_PEAK * smoothstep01(t / THRUST_WINDUP_END)
    } else if t < THRUST_STRIKE_END {
        let strike_t = (t - THRUST_WINDUP_END) / (THRUST_STRIKE_END - THRUST_WINDUP_END);
        // Cubic ease-in: accelerates hard into impact
        let ease = strike_t * strike_t * strike_t;
        THRUST_WINDUP_PEAK + THRUST_STRIKE_TRAVEL * ease
    } else {
        let recover_t = (t - THRUST_STRIKE_END) / (1.0 - THRUST_STRIKE_END);
        THRUST_STRIKE_PEAK * (1.0 - smoothstep01(recover_t))
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
