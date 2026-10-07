use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use crate::error::PluginError;
use crate::types::PluginId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    plugin_id: PluginId,
    values: HashMap<String, serde_json::Value>,
    schema: Option<serde_json::Value>,
    config_path: PathBuf,
}

impl PluginConfig {
    pub fn new(
        plugin_id: PluginId,
        schema: Option<serde_json::Value>,
        config_path: PathBuf,
    ) -> Self {
        Self {
            plugin_id,
            values: HashMap::new(),
            schema,
            config_path,
        }
    }

    pub fn load(
        plugin_id: PluginId,
        schema: Option<serde_json::Value>,
        config_path: PathBuf,
    ) -> Result<Self, PluginError> {
        let mut config = Self::new(plugin_id, schema, config_path);
        if config.config_path.exists() {
            let content = std::fs::read_to_string(&config.config_path)?;
            let loaded: HashMap<String, serde_json::Value> = toml::from_str(&content)
                .map_err(|e| PluginError::Serialization(e.to_string()))?;
            config.values = loaded;
        }
        Ok(config)
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.values.get(key)
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(|v| v.as_str())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| v.as_bool())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.values.get(key).and_then(|v| v.as_i64())
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.values.get(key).and_then(|v| v.as_f64())
    }

    pub fn set(&mut self, key: &str, value: serde_json::Value) -> Result<(), PluginError> {
        self.validate_against_schema(key, &value)?;
        self.values.insert(key.to_string(), value);
        Ok(())
    }

    pub fn set_string(&mut self, key: &str, value: &str) -> Result<(), PluginError> {
        self.set(key, serde_json::Value::String(value.to_string()))
    }

    pub fn set_bool(&mut self, key: &str, value: bool) -> Result<(), PluginError> {
        self.set(key, serde_json::Value::Bool(value))
    }

    pub fn set_i64(&mut self, key: &str, value: i64) -> Result<(), PluginError> {
        self.set(key, serde_json::Value::Number(value.into()))
    }

    pub fn set_f64(&mut self, key: &str, value: f64) -> Result<(), PluginError> {
        serde_json::Number::from_f64(value)
            .map(|n| self.set(key, serde_json::Value::Number(n)))
            .unwrap_or_else(|| {
                Err(PluginError::Serialization(format!(
                    "invalid float value for key: {}",
                    key
                )))
            })
    }

    pub fn save(&self) -> Result<(), PluginError> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(&self.values)
            .map_err(|e| PluginError::Serialization(e.to_string()))?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn keys(&self) -> Vec<&str> {
        self.values.keys().map(|s| s.as_str()).collect()
    }

    pub fn schema(&self) -> Option<&serde_json::Value> {
        self.schema.as_ref()
    }

    pub fn all_values(&self) -> &HashMap<String, serde_json::Value> {
        &self.values
    }

    fn validate_against_schema(
        &self,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<(), PluginError> {
        let schema = match &self.schema {
            Some(s) => s,
            None => return Ok(()),
        };

        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .ok_or_else(|| {
                PluginError::Serialization("invalid config_schema: missing properties".into())
            })?;

        let prop_schema = match props.get(key) {
            Some(ps) => ps,
            None => {
                return Err(PluginError::Serialization(format!(
                    "config key not in schema: {}",
                    key
                )));
            }
        };

        let expected_type = prop_schema.get("type").and_then(|t| t.as_str());

        if let Some(t) = expected_type {
            let valid = match t {
                "string" => value.is_string(),
                "boolean" => value.is_boolean(),
                "integer" => value.is_i64() || value.is_u64(),
                "number" => value.is_number(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => true,
            };
            if !valid {
                return Err(PluginError::Serialization(format!(
                    "config key {}: expected type {}, got {}",
                    key,
                    t,
                    value
                )));
            }
        }

        if let Some(enum_vals) = prop_schema.get("enum").and_then(|e| e.as_array()) {
            let valid = enum_vals.iter().any(|v| v == value);
            if !valid {
                return Err(PluginError::Serialization(format!(
                    "config key {}: value not in enum",
                    key
                )));
            }
        }

        Ok(())
    }
}

pub struct PluginConfigManager {
    configs: HashMap<PluginId, PluginConfig>,
    base_dir: PathBuf,
}

impl PluginConfigManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            configs: HashMap::new(),
            base_dir,
        }
    }

    pub fn default_dir() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("~"))
            .join(".orbit")
            .join("plugin-configs")
    }

    pub fn load_or_create(
        &mut self,
        plugin_id: PluginId,
        schema: Option<serde_json::Value>,
    ) -> Result<&PluginConfig, PluginError> {
        let config_path = self
            .base_dir
            .join(format!("{}.toml", plugin_id.to_path_segment()));

        let config = PluginConfig::load(plugin_id.clone(), schema, config_path)?;
        self.configs.insert(plugin_id.clone(), config);
        Ok(self.configs.get(&plugin_id).unwrap())
    }

    pub fn get(&self, plugin_id: &PluginId) -> Option<&PluginConfig> {
        self.configs.get(plugin_id)
    }

    pub fn get_mut(&mut self, plugin_id: &PluginId) -> Option<&mut PluginConfig> {
        self.configs.get_mut(plugin_id)
    }

    pub fn save(&self, plugin_id: &PluginId) -> Result<(), PluginError> {
        self.configs
            .get(plugin_id)
            .ok_or_else(|| {
                PluginError::Serialization(format!("config not found for plugin: {}", plugin_id))
            })?
            .save()
    }

    pub fn save_all(&self) -> Result<(), PluginError> {
        for config in self.configs.values() {
            config.save()?;
        }
        Ok(())
    }

    pub fn list_plugins(&self) -> Vec<&PluginId> {
        self.configs.keys().collect()
    }
}

impl Default for PluginConfigManager {
    fn default() -> Self {
        Self::new(Self::default_dir())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "api_key": { "type": "string" },
                "model": { "type": "string", "enum": ["claude-3-opus", "claude-3-sonnet"] },
                "max_tokens": { "type": "integer" },
                "enabled": { "type": "boolean" }
            }
        })
    }

    #[test]
    fn set_and_get_string() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        config.set_string("api_key", "sk-test-123").unwrap();
        assert_eq!(config.get_string("api_key"), Some("sk-test-123"));
    }

    #[test]
    fn set_and_get_bool() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        config.set_bool("enabled", true).unwrap();
        assert_eq!(config.get_bool("enabled"), Some(true));
    }

    #[test]
    fn set_and_get_i64() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        config.set_i64("max_tokens", 4096).unwrap();
        assert_eq!(config.get_i64("max_tokens"), Some(4096));
    }

    #[test]
    fn schema_validation_rejects_wrong_type() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        let result = config.set_bool("api_key", true);
        assert!(result.is_err());
    }

    #[test]
    fn schema_validation_rejects_unknown_key() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        let result = config.set_string("unknown_key", "value");
        assert!(result.is_err());
    }

    #[test]
    fn schema_validation_rejects_invalid_enum() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        let result = config.set_string("model", "gpt-4");
        assert!(result.is_err());
    }

    #[test]
    fn schema_validation_accepts_valid_enum() {
        let schema = make_schema();
        let mut config = PluginConfig::new(
            PluginId("test/plugin".into()),
            Some(schema),
            PathBuf::from("/tmp/test-config.toml"),
        );
        let result = config.set_string("model", "claude-3-opus");
        assert!(result.is_ok());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("orbit-test-configs");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let schema = make_schema();
        let config_path = dir.join("test_plugin.toml");

        {
            let mut config = PluginConfig::new(
                PluginId("test/plugin".into()),
                Some(schema.clone()),
                config_path.clone(),
            );
            config.set_string("api_key", "sk-test").unwrap();
            config.set_bool("enabled", true).unwrap();
            config.save().unwrap();
        }

        let loaded = PluginConfig::load(
            PluginId("test/plugin".into()),
            Some(schema),
            config_path.clone(),
        )
        .unwrap();
        assert_eq!(loaded.get_string("api_key"), Some("sk-test"));
        assert_eq!(loaded.get_bool("enabled"), Some(true));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
