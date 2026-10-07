use serde::{Deserialize, Serialize};

/// Plugin identifier: "<namespace>/<name>"
/// e.g. "orbit-ai/nested-agent"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub String);

impl PluginId {
    pub fn namespace(&self) -> &str {
        self.0.split('/').next().unwrap_or("")
    }

    pub fn name(&self) -> &str {
        self.0.split('/').nth(1).unwrap_or("")
    }

    /// Filesystem-safe form: "<namespace>_<name>"
    pub fn to_path_segment(&self) -> String {
        format!("{}_{}", self.namespace(), self.name())
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique ID for a plugin surface instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfaceId(pub u64);

/// Unique ID for a hook subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub u64);

/// Identifies the context (Session/Space/Pane/Agent) a plugin is bound to
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextId {
    Session,
    Space(orbt_protocol::SpaceId),
    Pane(orbt_protocol::PaneId),
    Agent(orbt_protocol::AgentId),
}

/// Semantic version
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Version(pub semver::Version);

impl Version {
    pub fn parse(s: &str) -> Result<Self, semver::Error> {
        Ok(Self(semver::Version::parse(s)?))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
