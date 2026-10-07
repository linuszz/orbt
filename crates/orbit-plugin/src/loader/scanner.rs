use std::path::PathBuf;
use crate::error::LoadError;
use crate::manifest::Manifest;

pub struct PluginScanner {
    base_dir: PathBuf,
}

impl PluginScanner {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn scan(&self) -> Result<Vec<(PathBuf, Manifest)>, LoadError> {
        let mut results = Vec::new();
        if !self.base_dir.exists() {
            return Ok(results);
        }
        for entry in std::fs::read_dir(&self.base_dir).map_err(|e| LoadError::ManifestNotFound {
            path: format!("{}: {}", self.base_dir.display(), e),
        })? {
            let entry = entry.map_err(|e| LoadError::ManifestNotFound {
                path: format!("read_dir error: {}", e),
            })?;
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            for version_entry in std::fs::read_dir(&plugin_dir).map_err(|e| LoadError::ManifestNotFound {
                path: format!("{}: {}", plugin_dir.display(), e),
            })? {
                let version_entry = version_entry.map_err(|e| LoadError::ManifestNotFound {
                    path: format!("read_dir error: {}", e),
                })?;
                let version_dir = version_entry.path();
                let manifest_path = version_dir.join("manifest.json");
                if manifest_path.exists() {
                    let content = std::fs::read_to_string(&manifest_path).map_err(|e| LoadError::ManifestNotFound {
                        path: format!("{}: {}", manifest_path.display(), e),
                    })?;
                    let manifest = Manifest::parse(&content).map_err(|e| LoadError::ManifestInvalidJson(e.to_string()))?;
                    results.push((version_dir, manifest));
                }
            }
        }
        Ok(results)
    }
}
