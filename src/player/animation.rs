use std::f32::consts::PI;

use bevy::prelude::*;

use crate::combat::{HitPoints, smoothstep01};

use super::{
    DeathAnim, Dodge, JointRest, KnightAnimator, KnightJoint, MoveTarget, Player, PlayerCombat,
    DEATH_ANIM_DURATION,
};

pub(super) fn animate_knight(
    time: Res<Time>,
    mut player: Single<
        (
            &MoveTarget,
            &Dodge,
            &HitPoints,
            &mut KnightAnimator,
            &mut PlayerCombat,
        ),
        With<Player>,
    >,
    mut joints: Query<(&KnightJoint, &JointRest, &mut Transform)>,
) {
    let (move_target, dodge, hp, ref mut animator, ref mut combat) = *player;

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

    if move_target.position.is_some() {
        animator.walk_phase += time.delta_secs() * 8.0;
        animator.walk_phase %= 100.0 * std::f32::consts::TAU;
    }

    let moving = move_target.position.is_some();
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
        smoothstep01(dodge_t) * PI * 2.0
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

        match joint {
            KnightJoint::Hips => {
                transform.translation.y += walk_bob;
                transform.rotation *= Quat::from_rotation_x(-sword_strike * 0.04)
                    * Quat::from_rotation_y(
                        walk_swing * 0.05 - sword_windup * 0.06 + sword_strike * 0.10,
                    );
            }
            KnightJoint::Chest => {
                transform.translation.y += walk_bob * 0.4;
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 0.16 - sword_strike * 0.28)
                        * Quat::from_rotation_y(
                            -walk_swing * 0.08 - sword_windup * 0.10 + sword_strike * 0.12,
                        )
                        * Quat::from_rotation_z(-sword_windup * 0.04 + sword_strike * 0.06);
            }
            KnightJoint::Head => {
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 0.04 - sword_strike * 0.05)
                        * Quat::from_rotation_y(sword_windup * 0.03 - sword_strike * 0.05);
            }
            KnightJoint::LeftArm => {
                transform.rotation *=
                    Quat::from_rotation_x(
                        -walk_swing * 0.45 + sword_windup * 0.08 - sword_strike * 0.04,
                    ) * Quat::from_rotation_z(sword_windup * 0.04 + sword_strike * 0.06);
            }
            KnightJoint::RightArm => {
                transform.rotation *=
                    Quat::from_rotation_x(
                        walk_swing * 0.12 + sword_windup * 1.86 - sword_strike * 2.48,
                    ) * Quat::from_rotation_y(sword_windup * 0.22 - sword_strike * 0.16)
                        * Quat::from_rotation_z(sword_windup * 0.10 - sword_strike * 0.18);
            }
            KnightJoint::LeftLeg => {
                transform.rotation *= Quat::from_rotation_x(walk_swing * 0.55);
            }
            KnightJoint::RightLeg => {
                transform.rotation *= Quat::from_rotation_x(-walk_swing * 0.55);
            }
            KnightJoint::Sword => {
                transform.rotation *=
                    Quat::from_rotation_x(sword_windup * 1.22 - sword_strike * 1.72)
                        * Quat::from_rotation_y(-sword_windup * 0.06 + sword_strike * 0.08)
                        * Quat::from_rotation_z(-sword_windup * 0.04 - sword_strike * 0.72);
            }
        }
    }
}

fn sword_swing_curve(timer: &Timer) -> f32 {
    if timer.finished() {
        return 0.0;
    }

    let duration = timer.duration().as_secs_f32();
    if duration <= 0.0 {
        return 0.0;
    }

    let t = timer.elapsed_secs() / duration;
    if t < 0.24 {
        -0.68 * smoothstep01(t / 0.24)
    } else if t < 0.54 {
        let strike_t = (t - 0.24) / 0.30;
        -0.68 + 1.96 * smoothstep01(strike_t)
    } else {
        let recover_t = (t - 0.54) / 0.46;
        1.28 * (1.0 - smoothstep01(recover_t))
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
