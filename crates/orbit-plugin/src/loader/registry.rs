use std::collections::HashMap;
use crate::types::PluginId;
use crate::plugin::Plugin;

pub struct PluginRegistry {
    plugins: HashMap<PluginId, Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: PluginId, plugin: Box<dyn Plugin>) {
        self.plugins.insert(id, plugin);
    }

    pub fn get(&self, id: &PluginId) -> Option<&dyn Plugin> {
        self.plugins.get(id).map(|p| p.as_ref())
    }

    pub fn get_mut(&mut self, id: &PluginId) -> Option<&mut (dyn Plugin + '_)> {
        self.plugins.get_mut(id).map(|p| p.as_mut() as &mut dyn Plugin)
    }

    pub fn remove(&mut self, id: &PluginId) -> Option<Box<dyn Plugin>> {
        self.plugins.remove(id)
    }

    pub fn list(&self) -> Vec<&PluginId> {
        self.plugins.keys().collect()
    }
}
