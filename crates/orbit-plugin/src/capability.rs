use serde::{Deserialize, Serialize};
use crate::types::{PluginId, Version};

pub type Scope = (String, String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDecl {
    pub name: CapabilityName,
    #[serde(default)]
    pub scopes: Vec<Scope>,
    #[serde(default)]
    pub default_scope: Option<Scope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityName(pub String);

impl std::fmt::Display for CapabilityName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub plugin_id: PluginId,
    pub version: Version,
    pub capability: CapabilityName,
    pub scopes: Vec<Scope>,
    pub granted_at_ms: u64,
}
