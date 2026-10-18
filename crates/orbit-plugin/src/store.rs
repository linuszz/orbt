use crate::error::PluginError;
use crate::types::{PluginId, Version};
use std::path::{Path, PathBuf};

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

    fn validate_key(&self, key: &str) -> Result<PathBuf, PluginError> {
        if key.contains("..") || key.starts_with('/') {
            return Err(PluginError::StorePathOutOfScope {
                path: key.to_string(),
            });
        }
        let path = self.base_path.join(key);
        let canonical_base = self.base_path.canonicalize().unwrap_or_else(|_| self.base_path.clone());
        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !canonical_path.starts_with(&canonical_base) {
            return Err(PluginError::StorePathOutOfScope {
                path: key.to_string(),
            });
        }
        Ok(path)
    }
}

impl StoreApi for Store {
    fn read(&self, key: &str) -> Result<Option<bytes::Bytes>, PluginError> {
        let path = self.validate_key(key)?;
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        Ok(Some(bytes::Bytes::from(data)))
    }

    fn write(&self, key: &str, value: bytes::Bytes) -> Result<(), PluginError> {
        let path = self.validate_key(key)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, value)?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), PluginError> {
        let path = self.validate_key(key)?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, PluginError> {
        let dir = self.validate_key(prefix)?;
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

pub struct GuardedStore {
    inner: Store,
    read_whitelist: Vec<String>,
    write_whitelist: Vec<String>,
}

impl GuardedStore {
    pub fn new(
        store: Store,
        read_whitelist: Vec<String>,
        write_whitelist: Vec<String>,
    ) -> Self {
        Self {
            inner: store,
            read_whitelist,
            write_whitelist,
        }
    }

    pub fn into_inner(self) -> Store {
        self.inner
    }
}

impl StoreApi for GuardedStore {
    fn read(&self, key: &str) -> Result<Option<bytes::Bytes>, PluginError> {
        self.inner.read(key)
    }

    fn write(&self, key: &str, value: bytes::Bytes) -> Result<(), PluginError> {
        self.inner.write(key, value)
    }

    fn delete(&self, key: &str) -> Result<(), PluginError> {
        self.inner.delete(key)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, PluginError> {
        self.inner.list(prefix)
    }
}
