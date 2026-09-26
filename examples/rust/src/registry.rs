use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct ComponentRegistry {
    names: HashMap<&'static str, fn(&mut EntityCommands)>,
}

impl ComponentRegistry {
    /// Maps a YAML component name to the typed component `C`.
    pub fn register_typed<C: Component>(&mut self, name: &'static str) {
        self.names.insert(name, |_| {});
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }
}
