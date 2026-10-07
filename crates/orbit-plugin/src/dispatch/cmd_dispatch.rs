use crate::command::CommandInvocation;
use crate::error::PluginError;
use crate::types::PluginId;
use std::collections::HashMap;

pub struct CommandDispatchCoordinator {
    provided_commands: HashMap<String, PluginId>,
    used_commands: HashMap<PluginId, Vec<String>>,
}

impl CommandDispatchCoordinator {
    pub fn new() -> Self {
        Self {
            provided_commands: HashMap::new(),
            used_commands: HashMap::new(),
        }
    }

    pub fn register_provided(&mut self, name: String, plugin_id: PluginId) {
        self.provided_commands.insert(name, plugin_id);
    }

    pub fn register_used(&mut self, plugin_id: PluginId, command: String) {
        self.used_commands
            .entry(plugin_id)
            .or_default()
            .push(command);
    }

    pub fn find_provider(&self, name: &str) -> Option<&PluginId> {
        self.provided_commands.get(name)
    }

    pub fn is_command_available(&self, plugin_id: &PluginId, name: &str) -> bool {
        self.used_commands
            .get(plugin_id)
            .map(|cmds| cmds.iter().any(|c| c == name))
            .unwrap_or(false)
    }

    pub fn dispatch(&self, invocation: CommandInvocation) -> Result<(), PluginError> {
        let name = invocation.name.0.clone();
        if let Some(provider) = self.find_provider(&name) {
            tracing::debug!(
                command = %name,
                provider = %provider,
                "routing command to plugin"
            );
            Ok(())
        } else {
            Err(PluginError::CommandNotFound { name })
        }
    }

    pub fn dispatch_to_baseline(&self, invocation: CommandInvocation) -> Result<(), PluginError> {
        tracing::debug!(command = %invocation.name, "dispatching to baseline");
        Ok(())
    }

    pub fn provided_count(&self) -> usize {
        self.provided_commands.len()
    }

    pub fn used_count(&self) -> usize {
        self.used_commands.values().map(|v| v.len()).sum()
    }
}

impl Default for CommandDispatchCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_find_provider() {
        let mut coord = CommandDispatchCoordinator::new();
        let pid = PluginId("test/plugin".into());
        coord.register_provided("test.cmd".into(), pid.clone());
        assert_eq!(coord.find_provider("test.cmd"), Some(&pid));
        assert_eq!(coord.find_provider("unknown"), None);
    }

    #[test]
    fn dispatch_unknown_command_fails() {
        let coord = CommandDispatchCoordinator::new();
        let result = coord.dispatch(CommandInvocation {
            name: crate::command::CommandName("unknown".into()),
            args: serde_json::json!({}),
        });
        assert!(matches!(result, Err(PluginError::CommandNotFound { .. })));
    }

    #[test]
    fn is_command_available() {
        let mut coord = CommandDispatchCoordinator::new();
        let pid = PluginId("test/plugin".into());
        coord.register_used(pid.clone(), "pane.send-text".into());
        assert!(coord.is_command_available(&pid, "pane.send-text"));
        assert!(!coord.is_command_available(&pid, "unknown.cmd"));
    }
}
