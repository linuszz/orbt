use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    pub topic: String,
    pub payload: serde_json::Value,
    pub timestamp_ms: u64,
    pub source_plugin: String,
}

pub trait Bus: Send + Sync {
    fn publish(&self, msg: BusMessage) -> Result<(), crate::error::PluginError>;
    fn subscribe(&self, topic: &str) -> Result<(), crate::error::PluginError>;
}
