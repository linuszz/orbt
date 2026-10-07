use serde::{Deserialize, Serialize};
use crate::error::PluginError;
use crate::types::SurfaceId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceDecl {
    pub kind: SurfaceKind,
    pub id: String,
    #[serde(default = "default_priority")]
    pub priority: u8,
    #[serde(default)]
    pub min_size: Option<Size>,
    #[serde(default)]
    pub max_size: Option<Size>,
    #[serde(default)]
    pub resize_policy: ResizePolicy,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

fn default_priority() -> u8 { 50 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SurfaceKind {
    PaneOverlay,
    PaneTitle,
    PaneBorderBadge,
    TabBarSlot,
    TabBarBadge,
    StatusBarSlot,
    ExtensionRail,
    PluginDock,
    AgentMonitorDock,
    SubcardSlot,
    Modal,
    FloatingChip,
    ContextMenuItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizePolicy {
    Stretch,
    Scroll,
    Clip,
}

impl Default for ResizePolicy {
    fn default() -> Self { Self::Stretch }
}

pub struct Surface {
    pub id: SurfaceId,
    pub decl: SurfaceDecl,
}

pub struct SurfaceHandle<'a> {
    pub(crate) surface_id: SurfaceId,
    pub(crate) ctx: &'a crate::context::RuntimeContext<'a>,
}

impl<'a> SurfaceHandle<'a> {
    pub fn render(&self, _frame: crate::render::Frame) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SurfaceEvent {
    Click { x: u16, y: u16, button: MouseButton },
    Key { key: crate::hook::KeySpec },
    Scroll { delta: i16 },
    Focus,
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}
