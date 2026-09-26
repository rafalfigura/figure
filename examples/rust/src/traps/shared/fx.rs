//! Throwaway visuals: debris and sparks.

use bevy::prelude::*;

pub fn burst(commands: &mut Commands, at: Vec3, count: usize, phase: &crate::core::Phase) {
    if *phase != crate::core::Phase::Playing {
        return;
    }
    for _ in 0..count {
        commands.spawn(Transform::from_translation(at));
    }
}
