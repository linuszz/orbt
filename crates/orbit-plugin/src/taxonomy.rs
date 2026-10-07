use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Taxonomy {
    pub hook_topics: HookTaxonomy,
    pub command_names: CommandTaxonomy,
    pub capability_names: CapabilityTaxonomy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookTaxonomy {
    pub topics: HashMap<String, HookTopicSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookTopicSpec {
    pub mode: String,
    pub delivery: String,
    #[serde(default)]
    pub filterable_fields: Vec<String>,
    #[serde(default)]
    pub allowed_scopes: Vec<String>,
    #[serde(default)]
    pub default_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTaxonomy {
    pub commands: HashMap<String, CommandSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    #[serde(default)]
    pub allowed_scopes: Vec<String>,
    #[serde(default)]
    pub default_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityTaxonomy {
    pub capabilities: HashMap<String, CapabilitySpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySpec {
    #[serde(default)]
    pub allowed_scopes: Vec<String>,
    #[serde(default)]
    pub default_scope: Option<String>,
    #[serde(default)]
    pub enabled_surface: Option<String>,
}

impl Taxonomy {
    pub fn embedded() -> Result<Self, serde_json::Error> {
        let hook_topics: HookTaxonomy = serde_json::from_str(
            include_str!("../taxonomy/hook-topics.json")
        )?;
        let command_names: CommandTaxonomy = serde_json::from_str(
            include_str!("../taxonomy/command-names.json")
        )?;
        let capability_names: CapabilityTaxonomy = serde_json::from_str(
            include_str!("../taxonomy/capability-names.json")
        )?;
        Ok(Self {
            hook_topics,
            command_names,
            capability_names,
        })
    }

    pub fn is_valid_topic(&self, topic: &str) -> bool {
        self.hook_topics.topics.contains_key(topic)
    }

    pub fn is_valid_command(&self, command: &str) -> bool {
        self.command_names.commands.contains_key(command)
    }

    pub fn is_valid_capability(&self, capability: &str) -> bool {
        self.capability_names.capabilities.contains_key(capability)
    }
}
