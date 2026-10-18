use orbit_plugin::loader::PluginScanner;
use orbit_plugin::PluginConfig;
use std::path::PathBuf;

#[test]
fn test_plugin_scanner_with_manifest() {
    let scanner = PluginScanner::new(PathBuf::from("/tmp/test-orbit-modules"));
    let result = scanner.scan();
    
    assert!(result.is_ok(), "scan failed: {:?}", result.err());
    let plugins = result.unwrap();
    assert_eq!(plugins.len(), 1, "expected 1 plugin, got {}", plugins.len());
    
    let (_, manifest) = &plugins[0];
    assert_eq!(manifest.id.to_string(), "test/plugin");
    assert_eq!(manifest.version.to_string(), "1.0.0");
}

#[test]
fn test_plugin_config_load() {
    let config_path = PathBuf::from("/tmp/test-orbit-modules/test-plugin/1.0.0/config.toml");
    let config = PluginConfig::load(
        orbit_plugin::types::PluginId("test/plugin".into()),
        None,
        config_path,
    );
    
    assert!(config.is_ok(), "config load failed: {:?}", config.err());
}
