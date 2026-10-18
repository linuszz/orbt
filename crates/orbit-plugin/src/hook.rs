use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use bytes::Bytes;
use crate::types::ContextId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookSpec {
    pub topic: HookTopic,
    #[serde(default)]
    pub filter: Option<Filter>,
    #[serde(default)]
    pub mode: HookMode,
    #[serde(default)]
    pub delivery: Delivery,
    #[serde(default)]
    pub buffer: Option<BufferPolicy>,
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 { 50 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookTopic {
    #[serde(rename = "pane.output")]
    PaneOutput,
    #[serde(rename = "pane.output.match")]
    PaneOutputMatch,
    #[serde(rename = "pane.exit")]
    PaneExit,
    #[serde(rename = "pane.focus")]
    PaneFocus,
    #[serde(rename = "pane.resize")]
    PaneResize,
    #[serde(rename = "tab.created")]
    TabCreated,
    #[serde(rename = "tab.closed")]
    TabClosed,
    #[serde(rename = "window.layout-changed")]
    WindowLayoutChanged,
    #[serde(rename = "space.activated")]
    SpaceActivated,
    #[serde(rename = "session.started")]
    SessionStarted,
    #[serde(rename = "session.ending")]
    SessionEnding,
    #[serde(rename = "session.status")]
    SessionStatus,
    #[serde(rename = "agent.detected")]
    AgentDetected,
    #[serde(rename = "agent.state")]
    AgentState,
    #[serde(rename = "agent.task-started")]
    AgentTaskStarted,
    #[serde(rename = "agent.task-finished")]
    AgentTaskFinished,
    #[serde(rename = "agent.tool-called")]
    AgentToolCalled,
    #[serde(rename = "key.event")]
    KeyEvent,
    #[serde(rename = "mouse.event")]
    MouseEvent,
    #[serde(rename = "plugin.loaded")]
    PluginLoaded,
    #[serde(rename = "plugin.unloaded")]
    PluginUnloaded,
    #[serde(rename = "plugin.errored")]
    PluginErrored,
    #[serde(rename = "plugin.deactivated")]
    PluginDeactivated,
    #[serde(rename = "bus.message")]
    BusMessage,
    #[serde(rename = "config.changed")]
    ConfigChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub fields: HashMap<String, FilterValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterValue {
    Str(String),
    StrSet(HashSet<String>),
    Wildcard,
    Regex(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookMode {
    Stream,
    Edge,
}

impl Default for HookMode {
    fn default() -> Self { Self::Edge }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    Sync,
    Async,
}

impl Default for Delivery {
    fn default() -> Self { Self::Async }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BufferPolicy {
    Bounded { size: usize, drop: DropPolicy },
    Unbounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DropPolicy {
    DropOldest,
    DropNewest,
}

#[derive(Debug, Clone)]
pub struct StreamEvent {
    pub topic: HookTopic,
    pub payload: Bytes,
    pub timestamp_ms: u64,
    pub source_ctx: ContextId,
}

#[derive(Debug, Clone)]
pub struct EdgeEvent {
    pub topic: HookTopic,
    pub payload: Bytes,
    pub timestamp_ms: u64,
    pub source_ctx: ContextId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub kind: PatternKind,
    pub scope: Option<String>,
    pub owner_plugin: crate::types::PluginId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternKind {
    Regex(String),
    SubString(String),
    JsonPath { path: String, value: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySpec {
    pub key: String,
    #[serde(default)]
    pub modifiers: Vec<String>,
    #[serde(default)]
    pub when: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: KeySpec,
    pub timestamp_ms: u64,
    pub source_ctx: ContextId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consume {
    Pass,
    Stop,
}
