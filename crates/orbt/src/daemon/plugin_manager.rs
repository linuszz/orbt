use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use orbit_plugin::context::{InitContext, PluginApi, RuntimeContext};
use orbit_plugin::error::PluginError;
use orbit_plugin::loader::PluginScanner;
use orbit_plugin::manifest::{Manifest, Runtime};
use orbit_plugin::runtime::native::NativeRuntime;
use orbit_plugin::types::PluginId;
use orbt_protocol::ServerEvent;
use tokio::sync::{broadcast, oneshot};
use tracing::{debug, error, info, warn};

use crate::daemon::plugin_surfaces::broadcast_surface_frame;

enum PluginOp {
    Activate,
    Deactivate,
    Command {
        name: String,
        args: serde_json::Value,
        reply: Option<oneshot::Sender<Result<Option<serde_json::Value>, String>>>,
    },
    Shutdown,
}

#[derive(Clone)]
struct PluginEntry {
    tx: std::sync::mpsc::Sender<PluginOp>,
}

pub struct PluginManager {
    plugins: Mutex<HashMap<String, PluginEntry>>,
    latest_frames: Mutex<HashMap<u64, (PluginId, orbit_plugin::render::Frame)>>,
}

impl PluginManager {
    pub fn load_plugins(event_bus: broadcast::Sender<ServerEvent>) -> Arc<Self> {
        let manager = Arc::new(Self {
            plugins: Mutex::new(HashMap::new()),
            latest_frames: Mutex::new(HashMap::new()),
        });

        for modules_dir in search_dirs() {
            debug!("daemon scanning for plugins in: {}", modules_dir.display());
            if !modules_dir.exists() {
                continue;
            }
            let scanner = PluginScanner::new(modules_dir.clone());
            let found = match scanner.scan() {
                Ok(found) => found,
                Err(e) => {
                    warn!("plugin scan failed in {}: {}", modules_dir.display(), e);
                    continue;
                }
            };
            for (version_dir, manifest) in found {
                if manifest.runtime != Runtime::Native {
                    debug!(
                        "skipping non-native plugin {} (runtime {:?})",
                        manifest.id, manifest.runtime
                    );
                    continue;
                }
                let lib_path = version_dir.join(&manifest.entry);
                if !lib_path.exists() {
                    warn!(
                        "plugin {} entry missing: {}",
                        manifest.id,
                        lib_path.display()
                    );
                    continue;
                }
                match spawn_plugin_thread(
                    manifest.id.clone(),
                    lib_path,
                    event_bus.clone(),
                    Arc::clone(&manager),
                ) {
                    Ok((tx, manifest)) => {
                        let plugin_id = manifest.id.to_string();
                        info!("plugin loaded: {} v{}", plugin_id, manifest.version);
                        let auto_activate = load_plugin_config(&manifest)
                            .map(|cfg| cfg.get_bool("auto_activate").unwrap_or(false))
                            .unwrap_or(false);
                        manager.plugins.lock().unwrap().insert(
                            plugin_id.clone(),
                            PluginEntry { tx: tx.clone() },
                        );
                        if auto_activate {
                            info!("auto-activating plugin: {}", plugin_id);
                            let _ = tx.send(PluginOp::Activate);
                        }
                    }
                    Err(e) => {
                        error!("failed to load plugin {}: {}", manifest.id, e);
                    }
                }
            }
        }

        let count = manager.plugins.lock().unwrap().len();
        info!("plugin manager ready: {} plugin(s) loaded", count);
        manager
    }

    pub async fn command(
        &self,
        plugin_id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, String> {
        let entry = {
            let plugins = self.plugins.lock().unwrap();
            plugins.get(plugin_id).cloned()
        };
        let entry = match entry {
            Some(e) => e,
            None => return Err(format!("plugin not loaded: {}", plugin_id)),
        };
        let (tx, rx) = oneshot::channel();
        entry
            .tx
            .send(PluginOp::Command {
                name: name.to_string(),
                args,
                reply: Some(tx),
            })
            .map_err(|_| format!("plugin thread unavailable: {}", plugin_id))?;
        rx.await
            .map_err(|_| format!("plugin command reply dropped: {}", plugin_id))?
    }

    pub fn shutdown_all(&self) {
        let plugins = self.plugins.lock().unwrap();
        for (id, entry) in plugins.iter() {
            debug!("shutting down plugin: {}", id);
            let _ = entry.tx.send(PluginOp::Shutdown);
        }
    }

    pub fn snapshot_frames(&self) -> Vec<(u64, String, orbit_plugin::render::Frame)> {
        let frames = self.latest_frames.lock().unwrap();
        frames
            .iter()
            .map(|(id, (plugin_id, frame))| (*id, plugin_id.to_string(), frame.clone()))
            .collect()
    }

    fn retain_frame(&self, surface_id: u64, plugin_id: PluginId, frame: orbit_plugin::render::Frame) {
        self.latest_frames
            .lock()
            .unwrap()
            .insert(surface_id, (plugin_id, frame));
    }
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".orbit").join("modules"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join(".orbit").join("modules"));
    }
    if let Ok(custom) = std::env::var("ORBT_MODULES") {
        dirs.push(PathBuf::from(custom));
    }
    dirs
}

fn load_plugin_config(manifest: &Manifest) -> Option<orbit_plugin::PluginConfig> {
    let home = std::env::var("HOME").ok().map(PathBuf::from)?;
    let path = home
        .join(".orbit")
        .join("plugin-configs")
        .join(format!("{}.toml", manifest.id.to_path_segment()));
    orbit_plugin::PluginConfig::load(
        manifest.id.clone(),
        manifest.config_schema.clone(),
        path,
    )
    .ok()
}

struct HostApi {
    event_bus: broadcast::Sender<ServerEvent>,
    plugin_id: PluginId,
    manager: Arc<PluginManager>,
}

impl PluginApi for HostApi {
    fn dispatch(&self, _cmd: orbit_plugin::command::CommandInvocation) -> Result<(), PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "dispatch".into(),
            detail: "not implemented in P1".into(),
        })
    }

    fn request(
        &self,
        _cmd: orbit_plugin::command::CommandInvocation,
    ) -> Result<serde_json::Value, PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "request".into(),
            detail: "not implemented in P1".into(),
        })
    }

    fn publish(&self, _topic: &str, _payload: serde_json::Value) -> Result<(), PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "publish".into(),
            detail: "not implemented in P1".into(),
        })
    }

    fn render(
        &self,
        surface_id: orbit_plugin::types::SurfaceId,
        frame: &orbit_plugin::render::Frame,
    ) -> Result<(), PluginError> {
        self.manager
            .retain_frame(surface_id.0, self.plugin_id.clone(), frame.clone());
        broadcast_surface_frame(&self.event_bus, surface_id, &self.plugin_id, frame);
        Ok(())
    }

    fn store_read(&self, _key: &str) -> Result<Option<bytes::Bytes>, PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "store_read".into(),
            detail: "not implemented in P1".into(),
        })
    }

    fn store_write(&self, _key: &str, _value: bytes::Bytes) -> Result<(), PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "store_write".into(),
            detail: "not implemented in P1".into(),
        })
    }

    fn store_delete(&self, _key: &str) -> Result<(), PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "store_delete".into(),
            detail: "not implemented in P1".into(),
        })
    }

    fn store_list(&self, _prefix: &str) -> Result<Vec<String>, PluginError> {
        Err(PluginError::CommandRuntimeFailed {
            name: "store_list".into(),
            detail: "not implemented in P1".into(),
        })
    }
}

fn spawn_plugin_thread(
    plugin_id: PluginId,
    lib_path: PathBuf,
    event_bus: broadcast::Sender<ServerEvent>,
    manager: Arc<PluginManager>,
) -> Result<(std::sync::mpsc::Sender<PluginOp>, Manifest), String> {
    let manifest = read_manifest(&lib_path)?;
    let (tx, rx) = std::sync::mpsc::channel::<PluginOp>();
    let thread_id = plugin_id.to_string();
    std::thread::Builder::new()
        .name(format!("plugin-{}", thread_id))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_plugin(plugin_id, lib_path, event_bus, manager, rx);
            }));
            if result.is_err() {
                error!("plugin thread panicked: {}", thread_id);
            }
        })
        .map_err(|e| format!("spawn failed: {}", e))?;
    Ok((tx, manifest))
}

fn read_manifest(lib_path: &std::path::Path) -> Result<Manifest, String> {
    let manifest_path = lib_path
        .parent()
        .ok_or_else(|| "lib path has no parent".to_string())?
        .join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read {}: {}", manifest_path.display(), e))?;
    Manifest::parse(&content).map_err(|e| format!("parse manifest: {}", e))
}

fn run_plugin(
    plugin_id: PluginId,
    lib_path: PathBuf,
    event_bus: broadcast::Sender<ServerEvent>,
    manager: Arc<PluginManager>,
    rx: std::sync::mpsc::Receiver<PluginOp>,
) {
    let mut runtime = NativeRuntime::new();
    let mut plugin = match runtime.load_plugin(plugin_id.clone(), &lib_path) {
        Ok(p) => p,
        Err(e) => {
            error!("plugin load failed {}: {}", plugin_id, e);
            return;
        }
    };

    let api = HostApi {
        event_bus,
        plugin_id: plugin_id.clone(),
        manager,
    };

    let mut init_ctx = InitContext::new(&api);
    if let Err(e) = plugin.init(&mut init_ctx) {
        error!("plugin init failed {}: {}", plugin_id, e);
        return;
    }
    let (mut surfaces, mut subscriptions) = init_ctx.into_parts();
    info!(
        "plugin initialized: {} ({} surface(s), {} subscription(s))",
        plugin_id,
        surfaces.len(),
        subscriptions.len()
    );

    while let Ok(op) = rx.recv() {
        match op {
            PluginOp::Activate => {
                let mut ctx = RuntimeContext::new(&api, surfaces, subscriptions);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    plugin.on_activate(&mut ctx);
                }));
                (surfaces, subscriptions) = ctx.into_parts();
                if result.is_err() {
                    error!("plugin on_activate panicked: {}", plugin_id);
                    break;
                }
                info!("plugin activated: {}", plugin_id);
            }
            PluginOp::Deactivate => {
                let mut ctx = RuntimeContext::new(&api, surfaces, subscriptions);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    plugin.on_deactivate(&mut ctx);
                }));
                (surfaces, subscriptions) = ctx.into_parts();
                if result.is_err() {
                    error!("plugin on_deactivate panicked: {}", plugin_id);
                    break;
                }
                info!("plugin deactivated: {}", plugin_id);
            }
            PluginOp::Command { name, args, reply } => {
                let mut ctx = RuntimeContext::new(&api, surfaces, subscriptions);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    plugin.on_command(&mut ctx, &name, &args)
                }));
                (surfaces, subscriptions) = ctx.into_parts();
                let outcome = match result {
                    Ok(Ok(value)) => Ok(value),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => {
                        error!("plugin on_command panicked: {} {}", plugin_id, name);
                        break;
                    }
                };
                if let Some(reply) = reply {
                    let _ = reply.send(outcome);
                }
            }
            PluginOp::Shutdown => {
                plugin.shutdown();
                info!("plugin shutdown: {}", plugin_id);
                break;
            }
        }
    }
}
