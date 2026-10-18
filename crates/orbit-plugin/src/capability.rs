use serde::{Deserialize, Serialize};
use crate::error::PluginError;
use crate::types::{PluginId, Version};
use std::collections::HashMap;
use std::path::PathBuf;

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

pub struct CapabilityGrantManager {
    cache_dir: PathBuf,
    grants: HashMap<PluginId, Vec<CapabilityGrant>>,
}

impl CapabilityGrantManager {
    pub fn new() -> Self {
        let cache_dir = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("~"))
            .join(".orbit")
            .join("cache")
            .join("permissions");
        Self {
            cache_dir,
            grants: HashMap::new(),
        }
    }

    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            grants: HashMap::new(),
        }
    }

    pub fn check_grant(
        &self,
        plugin_id: &PluginId,
        capability: &CapabilityName,
        required_scope: &Scope,
    ) -> Result<(), PluginError> {
        let plugin_grants = self.grants.get(plugin_id).ok_or_else(|| {
            PluginError::CapabilityDenied {
                name: capability.to_string(),
            }
        })?;

        let grant = plugin_grants
            .iter()
            .find(|g| g.capability == *capability)
            .ok_or_else(|| PluginError::CapabilityDenied {
                name: capability.to_string(),
            })?;

        let scope_matches = grant.scopes.iter().any(|(k, v)| {
            k == &required_scope.0 && (v == &required_scope.1 || v == "any")
        });

        if scope_matches {
            Ok(())
        } else {
            Err(PluginError::CapabilityInvalidScope {
                name: capability.to_string(),
                scope: format!("{}={}", required_scope.0, required_scope.1),
            })
        }
    }

    pub fn grant(
        &mut self,
        plugin_id: PluginId,
        version: Version,
        capability: CapabilityName,
        scopes: Vec<Scope>,
    ) -> Result<(), PluginError> {
        let grant = CapabilityGrant {
            plugin_id: plugin_id.clone(),
            version,
            capability: capability.clone(),
            scopes: scopes.clone(),
            granted_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        let plugin_grants = self.grants.entry(plugin_id.clone()).or_default();
        plugin_grants.retain(|g| g.capability != capability);
        plugin_grants.push(grant);

        self.persist_grant(&plugin_id, &capability, &scopes)
    }

    pub fn revoke(
        &mut self,
        plugin_id: &PluginId,
        capability: &CapabilityName,
    ) -> Result<(), PluginError> {
        if let Some(grants) = self.grants.get_mut(plugin_id) {
            grants.retain(|g| g.capability != *capability);
        }
        self.remove_persisted_grant(plugin_id, capability)
    }

    pub fn load_from_cache(&mut self) -> Result<usize, PluginError> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }
        let mut count = 0;
        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                let content = std::fs::read_to_string(&path)?;
                let grant: CapabilityGrant = toml::from_str(&content)
                    .map_err(|e| PluginError::Serialization(e.to_string()))?;
                self.grants
                    .entry(grant.plugin_id.clone())
                    .or_default()
                    .push(grant);
                count += 1;
            }
        }
        Ok(count)
    }

    fn cache_file_path(&self, plugin_id: &PluginId, capability: &CapabilityName) -> PathBuf {
        let filename = format!(
            "{}@{}.toml",
            plugin_id.to_path_segment(),
            capability.0.replace('.', "_")
        );
        self.cache_dir.join(filename)
    }

    fn persist_grant(
        &self,
        plugin_id: &PluginId,
        capability: &CapabilityName,
        scopes: &[Scope],
    ) -> Result<(), PluginError> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let grant = CapabilityGrant {
            plugin_id: plugin_id.clone(),
            version: Version(semver::Version::new(0, 0, 0)),
            capability: capability.clone(),
            scopes: scopes.to_vec(),
            granted_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };
        let content = toml::to_string_pretty(&grant)
            .map_err(|e| PluginError::Serialization(e.to_string()))?;
        std::fs::write(self.cache_file_path(plugin_id, capability), content)?;
        Ok(())
    }

    fn remove_persisted_grant(
        &self,
        plugin_id: &PluginId,
        capability: &CapabilityName,
    ) -> Result<(), PluginError> {
        let path = self.cache_file_path(plugin_id, capability);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

impl Default for CapabilityGrantManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> CapabilityGrantManager {
        CapabilityGrantManager::with_cache_dir(PathBuf::from("/tmp/test-capabilities"))
    }

    #[test]
    fn grant_and_check() {
        let mut mgr = make_manager();
        let pid = PluginId("test/plugin".into());
        let cap = CapabilityName("command.pane-write".into());
        let scope: Scope = ("target".into(), "self".into());

        mgr.grant(
            pid.clone(),
            Version(semver::Version::new(1, 0, 0)),
            cap.clone(),
            vec![scope.clone()],
        )
        .unwrap();

        assert!(mgr.check_grant(&pid, &cap, &scope).is_ok());
    }

    #[test]
    fn check_denied_no_grant() {
        let mgr = make_manager();
        let pid = PluginId("test/plugin".into());
        let cap = CapabilityName("unknown.cap".into());
        let scope: Scope = ("target".into(), "self".into());

        let result = mgr.check_grant(&pid, &cap, &scope);
        assert!(matches!(result, Err(PluginError::CapabilityDenied { .. })));
    }

    #[test]
    fn check_scope_mismatch() {
        let mut mgr = make_manager();
        let pid = PluginId("test/plugin".into());
        let cap = CapabilityName("command.pane-write".into());

        mgr.grant(
            pid.clone(),
            Version(semver::Version::new(1, 0, 0)),
            cap.clone(),
            vec![("target".into(), "self".into())],
        )
        .unwrap();

        let result = mgr.check_grant(&pid, &cap, &("target".into(), "any".into()));
        assert!(matches!(result, Err(PluginError::CapabilityInvalidScope { .. })));
    }

    #[test]
    fn revoke_removes_grant() {
        let mut mgr = make_manager();
        let pid = PluginId("test/plugin".into());
        let cap = CapabilityName("command.pane-write".into());

        mgr.grant(
            pid.clone(),
            Version(semver::Version::new(1, 0, 0)),
            cap.clone(),
            vec![("target".into(), "self".into())],
        )
        .unwrap();

        mgr.revoke(&pid, &cap).unwrap();

        let result = mgr.check_grant(&pid, &cap, &("target".into(), "self".into()));
        assert!(matches!(result, Err(PluginError::CapabilityDenied { .. })));
    }
}
