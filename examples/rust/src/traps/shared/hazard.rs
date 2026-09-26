//! How traps hurt things: a [`Harm`] delivered as a [`Harmed`] event.

use bevy::prelude::*;

/// Damage and knockback dealt by one hit.
pub struct Harm {
    pub damage: f32,
    pub knockback: f32,
    from: Option<Vec3>,
}

impl Harm {
    /// A hit with no knockback.
    pub fn new(damage: f32) -> Self {
        Harm { damage, knockback: 0.0, from: None }
    }

    pub fn push(mut self, from: Vec3, knockback: f32) -> Self {
        self.from = Some(from);
        self.knockback = knockback;
        self
    }
}

/// Sent when a trap hits an entity.
#[derive(Event)]
pub struct Harmed {
    pub entity: Entity,
    pub harm: Harm,
}

/// A pressure plate position (tests only need it).
#[derive(Component)]
pub struct TriggerPlate(pub Vec3);

#[doc(hidden)]
pub fn debug_harm() -> Harm {
    Harm::new(1.0)
}
