use crate::command::CommandInvocation;
use crate::error::PluginError;
use std::collections::HashMap;

pub struct CommandDispatchCoordinator {
    provided_commands: HashMap<String, crate::types::PluginId>,
}

impl CommandDispatchCoordinator {
    pub fn new() -> Self {
        Self {
            provided_commands: HashMap::new(),
        }
    }

    pub fn register_provided(&mut self, name: String, plugin_id: crate::types::PluginId) {
        self.provided_commands.insert(name, plugin_id);
    }

    pub fn find_provider(&self, name: &str) -> Option<&crate::types::PluginId> {
        self.provided_commands.get(name)
    }

    pub fn dispatch(&self, _invocation: CommandInvocation) -> Result<(), PluginError> {
        Ok(())
    }
}
