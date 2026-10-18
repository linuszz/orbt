use crate::context::{InitContext, RuntimeContext, UpgradeContext};
use crate::error::{PluginError, UpgradeError};
use crate::hook::{StreamEvent, EdgeEvent};
use crate::surface::SurfaceEvent;
use crate::types::SurfaceId;

pub trait Plugin: Send {
    fn new() -> Self where Self: Sized;

    fn init(&mut self, ctx: &mut InitContext) -> Result<(), PluginError>;

    fn on_activate(&mut self, _ctx: &mut RuntimeContext) {}

    fn on_deactivate(&mut self, _ctx: &mut RuntimeContext) {}

    fn on_command(
        &mut self,
        _ctx: &mut RuntimeContext,
        cmd_name: &str,
        _args: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, PluginError> {
        Err(PluginError::CommandNotFound { name: cmd_name.into() })
    }

    fn on_surface_event(
        &mut self,
        _ctx: &mut RuntimeContext,
        _surface_id: SurfaceId,
        _event: SurfaceEvent,
    ) {}

    fn on_stream(&mut self, _ctx: &mut RuntimeContext, _event: StreamEvent) {}
    fn on_edge(&mut self, _ctx: &mut RuntimeContext, _event: EdgeEvent) {}
    fn on_sync(&mut self, _ctx: &mut RuntimeContext, _event: EdgeEvent) {}

    fn on_session_start(&mut self, _ctx: &mut RuntimeContext) {}
    fn on_session_end(&mut self, _ctx: &mut RuntimeContext) {}

    fn on_upgrade(
        &mut self,
        _ctx: &mut UpgradeContext,
        _old_version: &str,
        _new_version: &str,
    ) -> Result<(), UpgradeError> { Ok(()) }

    fn shutdown(&mut self) {}
}
