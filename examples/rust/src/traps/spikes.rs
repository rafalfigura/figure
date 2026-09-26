//! Pop-up Spikes: a floor plate whose spikes go down, rattle, then up on a cycle.

use bevy::prelude::*;
use serde::Deserialize;

use super::shared::{
    fx,
    hazard::{Harm, Harmed},
};
use crate::core::*;

pub(super) fn plugin(app: &mut App) {
    super::register::<Spikes>(app, "Spikes");
    app.add_observer(build)
        .add_systems(Update, run.run_if(gameplay_running));
}

/// Timed floor spikes: down -> warn -> up, forever.
/// YAML: { type: Spikes, down: 2.0, up: 1.0, damage: 12 }
#[derive(Component, Deserialize)]
pub struct Spikes {
    down: f32,
    up: f32,
    damage: f32,
    #[serde(skip)]
    t: f32,
}

fn build(event: On<Add, Spikes>, mut commands: Commands, phase: Res<State<Phase>>) {
    fx::burst(&mut commands, Vec3::ZERO, 3, phase.get());
}

fn run(mut spikes: Query<&mut Spikes>, time: Res<Time>, mut harmed: EventWriter<Harmed>) {
    for mut s in &mut spikes {
        s.t += time.delta_secs();
        if s.t > s.down + s.up {
            s.t = 0.0;
            let _ = Harm::new(s.damage);
        }
    }
}
