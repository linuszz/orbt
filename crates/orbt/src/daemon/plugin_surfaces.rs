use orbit_plugin::render::SurfaceGridManager;
use orbit_plugin::types::{PluginId, SurfaceId};
use orbit_plugin::PluginConfigManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use orbt_protocol::ServerEvent;

pub struct PluginSurfaceState {
    pub grid_manager: SurfaceGridManager,
    pub surface_owners: HashMap<SurfaceId, PluginId>,
    pub config_manager: PluginConfigManager,
}

impl PluginSurfaceState {
    pub fn new() -> Self {
        Self {
            grid_manager: SurfaceGridManager::new(),
            surface_owners: HashMap::new(),
            config_manager: PluginConfigManager::default(),
        }
    }

    pub fn register_surface(&mut self, surface_id: SurfaceId, plugin_id: PluginId) {
        self.surface_owners.insert(surface_id, plugin_id);
    }

    pub fn unregister_plugin_surfaces(&mut self, plugin_id: &PluginId) {
        let surfaces: Vec<SurfaceId> = self
            .surface_owners
            .iter()
            .filter(|(_, pid)| *pid == plugin_id)
            .map(|(sid, _)| *sid)
            .collect();
        for sid in surfaces {
            self.surface_owners.remove(&sid);
            self.grid_manager.remove_grid(&sid);
        }
    }

    pub fn owner_of(&self, surface_id: &SurfaceId) -> Option<&PluginId> {
        self.surface_owners.get(surface_id)
    }
}

impl Default for PluginSurfaceState {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedPluginSurfaces = Arc<RwLock<PluginSurfaceState>>;

pub fn broadcast_surface_frame(
    event_bus: &tokio::sync::broadcast::Sender<ServerEvent>,
    surface_id: SurfaceId,
    plugin_id: &PluginId,
    frame: &orbit_plugin::render::Frame,
) {
    let event = ServerEvent::SurfaceFrame {
        surface_id: surface_id.0,
        plugin_id: plugin_id.to_string(),
        width: frame.size.w,
        height: frame.size.h,
        cells: frame.buffer.clone(),
    };
    let _ = event_bus.send(event);
}
