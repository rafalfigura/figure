//! Builds the Bevy app and adds every plugin.

use bevy::prelude::*;

use crate::traps::TrapsPlugin;

/// Starts the game.
pub fn run() {
    App::new().add_plugins(TrapsPlugin).run();
}
