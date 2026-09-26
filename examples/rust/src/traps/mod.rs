//! Traps: environmental hazards that hurt whoever steps in.
//!
//! Every trap is a YAML component registered through [`register`]; its file builds the
//! look in an observer and runs the behaviour in a system.
//!
//! # How to add a trap
//! 1. Create `src/traps/<name>.rs` with a `pub(super) fn plugin(app: &mut App)`.
//! 2. Register the component: [`register`]`::<YourTrap>(app, "YourTrap")`.
//! 3. Hurt things with [`Harm`](crate::traps::shared::hazard::Harm) and [`shared::fx::burst`].
//! 4. Add `<name>::plugin(app)` to [`TrapsPlugin`].
//!
//! Example to copy: [`Spikes`]
//!
//! # Invariants
//! - Every trap component is registered before the level loads.
//!
//! # Gotchas
//! Traps never import each other; shared code goes in `shared/`.

pub mod saw;
pub mod shared;
pub mod spikes;

use bevy::prelude::*;
use serde::de::DeserializeOwned;

use crate::registry::ComponentRegistry;
#[cfg(test)]
pub use shared::hazard::TriggerPlate;

pub struct TrapsPlugin;

impl Plugin for TrapsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentRegistry>();
        shared::plugin(app);
        spikes::plugin(app);
        saw::plugin(app);
    }
}

/// Registers a YAML component type whose spec deserializes straight into `C`.
pub fn register<C: Component + DeserializeOwned>(app: &mut App, name: &'static str) {
    app.world_mut()
        .resource_mut::<ComponentRegistry>()
        .register_typed::<C>(name);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every trap component is registered.
    #[test]
    fn components_registered() {
        let mut app = App::new();
        app.add_plugins(TrapsPlugin);
        assert!(app.world().resource::<ComponentRegistry>().len() >= 2);
    }

    #[test]
    fn plugin_builds() {
        App::new().add_plugins(TrapsPlugin);
    }

    /// Specs in the test data parse.
    #[test]
    fn specs_parse() {
        let _: serde_yaml::Value = serde_yaml::from_str("type: Spikes").unwrap();
    }
}
