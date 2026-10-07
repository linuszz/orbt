use serde::{Deserialize, Serialize};
use crate::types::{PluginId, Version};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: PluginId,
    pub version: Version,
    pub runtime: Runtime,
    pub entry: String,
    pub orbit_version: String,
    #[serde(default)]
    pub contexts: Vec<Context>,
    #[serde(default)]
    pub auto_bind: AutoBind,
    #[serde(default)]
    pub thread_affinity: ThreadAffinity,
    #[serde(default)]
    pub upgrade_policy: UpgradePolicy,
    #[serde(default)]
    pub performance_hint: PerformanceHint,
    #[serde(default)]
    pub network_hosts: Vec<String>,
    #[serde(default)]
    pub surfaces: Vec<SurfaceDecl>,
    #[serde(default)]
    pub hooks: Vec<HookDecl>,
    #[serde(default)]
    pub commands_used: Vec<String>,
    #[serde(default)]
    pub commands_provided: Vec<CommandProvidedDecl>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityDecl>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Wasm,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Context {
    Session,
    Space,
    Pane,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoBind {
    None,
    ActivePane,
    ActiveSpace,
    SessionStart,
}

impl Default for AutoBind {
    fn default() -> Self { Self::None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadAffinity {
    Pinned,
    None,
}

impl Default for ThreadAffinity {
    fn default() -> Self { Self::Pinned }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpgradePolicy {
    Silent,
    Strict,
}

impl Default for UpgradePolicy {
    fn default() -> Self { Self::Silent }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PerformanceHint {
    High,
    Medium,
    Low,
}

impl Default for PerformanceHint {
    fn default() -> Self { Self::Medium }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceDecl {
    pub kind: SurfaceKind,
    pub id: String,
    #[serde(default = "default_priority")]
    pub priority: u8,
    #[serde(default)]
    pub min_size: Option<Size>,
    #[serde(default)]
    pub max_size: Option<Size>,
    #[serde(default)]
    pub resize_policy: ResizePolicy,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

fn default_priority() -> u8 { 50 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SurfaceKind {
    PaneOverlay,
    PaneTitle,
    PaneBorderBadge,
    TabBarSlot,
    TabBarBadge,
    StatusBarSlot,
    ExtensionRail,
    PluginDock,
    AgentMonitorDock,
    SubcardSlot,
    Modal,
    FloatingChip,
    ContextMenuItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizePolicy {
    Stretch,
    Scroll,
    Clip,
}

impl Default for ResizePolicy {
    fn default() -> Self { Self::Stretch }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDecl {
    pub topic: String,
    #[serde(default)]
    pub filter: Option<serde_json::Value>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub delivery: Option<String>,
    #[serde(default)]
    pub buffer: Option<String>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandProvidedDecl {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub args_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDecl {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub default_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub optional: bool,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
