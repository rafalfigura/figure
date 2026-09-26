//! Building blocks several traps share.

pub mod fx;
pub mod hazard;

use bevy::prelude::*;

pub use fx::burst;

pub(super) fn plugin(app: &mut App) {
    app.add_event::<hazard::Harmed>();
}
