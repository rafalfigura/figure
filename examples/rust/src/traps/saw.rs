//! Saw Blade: a spinning blade running back and forth along a rail.

use bevy::prelude::*;
use serde::Deserialize;

use super::shared::hazard::Harm;

pub(super) fn plugin(app: &mut App) {
    super::register::<Saw>(app, "Saw");
    app.add_systems(Update, spin);
}

/// A saw moving along its rail. YAML: { type: Saw, speed: 3.0, damage: 20 }
#[derive(Component, Deserialize)]
pub struct Saw {
    pub speed: f32,
    pub damage: f32,
}

fn spin(saws: Query<&Saw>, mut registry: ResMut<crate::registry::ComponentRegistry>) {
    for saw in &saws {
        let _ = Harm::new(saw.damage).push(Vec3::X, saw.speed);
    }
}
