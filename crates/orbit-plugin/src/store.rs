use crate::error::PluginError;
use crate::types::{PluginId, Version};
use std::path::PathBuf;

pub struct Store {
    plugin_id: PluginId,
    version: Version,
    base_path: PathBuf,
}

pub trait StoreApi {
    fn read(&self, key: &str) -> Result<Option<bytes::Bytes>, PluginError>;
    fn write(&self, key: &str, value: bytes::Bytes) -> Result<(), PluginError>;
    fn delete(&self, key: &str) -> Result<(), PluginError>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, PluginError>;
}

impl Store {
    pub fn new(plugin_id: PluginId, version: Version) -> Self {
        let base_path = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("~"))
            .join(".orbit")
            .join("modules")
            .join(plugin_id.to_path_segment())
            .join(version.to_string())
            .join("state");
        Self {
            plugin_id,
            version,
            base_path,
        }
    }

    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

impl StoreApi for Store {
    fn read(&self, key: &str) -> Result<Option<bytes::Bytes>, PluginError> {
        let path = self.base_path.join(key);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        Ok(Some(bytes::Bytes::from(data)))
    }

    fn write(&self, key: &str, value: bytes::Bytes) -> Result<(), PluginError> {
        let path = self.base_path.join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, value)?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), PluginError> {
        let path = self.base_path.join(key);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, PluginError> {
        let dir = self.base_path.join(prefix);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut keys = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            keys.push(format!("{}/{}", prefix, name));
        }
        keys.sort();
        Ok(keys)
    }
}
