pub mod app;
pub mod tui;

pub use app::{load_settings, App};
pub use tui::render;
pub use tui::theme;

/// Thin stub for the plugin config system (full orbit-plugin crate is Phase 4 / Mars).
pub mod orbit_plugin {
    #[derive(Clone, Default)]
    pub struct PluginConfig {
        pub id: String,
    }
    impl PluginConfig {
        pub fn load(
            _id: loader::ManifestId,
            _schema: Option<String>,
            _path: std::path::PathBuf,
        ) -> anyhow::Result<Self> {
            Ok(Self::default())
        }
        pub fn keys(&self) -> Vec<&str> {
            vec![]
        }
    }
    pub mod loader {
        pub struct PluginScanner;
        impl PluginScanner {
            pub fn new(_dir: std::path::PathBuf) -> Self {
                Self
            }
            pub fn scan(&self) -> anyhow::Result<Vec<(String, Manifest)>> {
                Ok(vec![])
            }
        }
        pub struct Manifest {
            pub id: ManifestId,
            pub config_schema: Option<String>,
        }
        #[derive(Clone)]
        pub struct ManifestId(pub String);
        impl ManifestId {
            pub fn to_path_segment(&self) -> &str {
                &self.0
            }
        }
        impl std::fmt::Display for ManifestId {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    }
}
