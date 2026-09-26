//! Game-wide state shared by every system.

use bevy::prelude::*;

/// True while the player is in control (not paused, not in a menu).
pub fn gameplay_running(state: Res<State<Phase>>) -> bool {
    *state.get() == Phase::Playing
}

/// Which screen the game is on.
#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Phase {
    #[default]
    Menu,
    Playing,
}
