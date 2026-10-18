use std::collections::HashMap;
use crate::error::PluginError;
use crate::types::{SurfaceId, SubscriptionId};
use crate::hook::HookSpec;
use crate::command::{CommandInvocation, CommandName, CommandProvidedDecl};
use crate::surface::{SurfaceDecl, SurfaceHandle, Surface};
use crate::store::Store;
use serde::Serialize;

pub struct InitContext<'a> {
    pub(crate) api: &'a dyn PluginApi,
    pub(crate) surfaces: HashMap<SurfaceId, Surface>,
    pub(crate) subscriptions: HashMap<SubscriptionId, HookSpec>,
}

impl<'a> InitContext<'a> {
    pub fn new(api: &'a dyn PluginApi) -> Self {
        Self {
            api,
            surfaces: HashMap::new(),
            subscriptions: HashMap::new(),
        }
    }

    pub fn into_parts(
        self,
    ) -> (
        HashMap<SurfaceId, Surface>,
        HashMap<SubscriptionId, HookSpec>,
    ) {
        (self.surfaces, self.subscriptions)
    }

    pub fn declare_surface(&mut self, decl: SurfaceDecl) -> Result<SurfaceId, PluginError> {
        let id = SurfaceId(self.surfaces.len() as u64);
        self.surfaces.insert(id, Surface { id, decl });
        Ok(id)
    }

    pub fn subscribe(&mut self, spec: HookSpec) -> Result<SubscriptionId, PluginError> {
        let id = SubscriptionId(self.subscriptions.len() as u64);
        self.subscriptions.insert(id, spec);
        Ok(id)
    }

    pub fn register_pattern(&mut self, _pattern: crate::hook::Pattern) -> Result<crate::hook::PatternId, PluginError> {
        Ok(crate::hook::PatternId(0))
    }

    pub fn declare_command_used(&mut self, _name: CommandName) -> Result<(), PluginError> {
        Ok(())
    }

    pub fn declare_command_provided(&mut self, _decl: CommandProvidedDecl) -> Result<(), PluginError> {
        Ok(())
    }

    pub fn register_key(
        &mut self,
        _spec: crate::hook::KeySpec,
        _handler: fn(&mut RuntimeContext, crate::hook::KeyEvent) -> crate::hook::Consume,
    ) -> Result<(), PluginError> {
        Ok(())
    }
}

pub struct RuntimeContext<'a> {
    pub(crate) api: &'a dyn PluginApi,
    pub(crate) surfaces: HashMap<SurfaceId, Surface>,
    pub(crate) subscriptions: HashMap<SubscriptionId, HookSpec>,
}

impl<'a> RuntimeContext<'a> {
    pub fn new(
        api: &'a dyn PluginApi,
        surfaces: HashMap<SurfaceId, Surface>,
        subscriptions: HashMap<SubscriptionId, HookSpec>,
    ) -> Self {
        Self {
            api,
            surfaces,
            subscriptions,
        }
    }

    pub fn into_parts(
        self,
    ) -> (
        HashMap<SurfaceId, Surface>,
        HashMap<SubscriptionId, HookSpec>,
    ) {
        (self.surfaces, self.subscriptions)
    }

    pub fn dispatch(&self, cmd: CommandInvocation) -> Result<(), PluginError> {
        self.api.dispatch(cmd)
    }

    pub fn request<R: serde::de::DeserializeOwned>(
        &self,
        cmd: CommandInvocation,
    ) -> Result<R, PluginError> {
        let value = self.api.request(cmd)?;
        serde_json::from_value(value).map_err(|e| PluginError::Serialization(e.to_string()))
    }

    pub fn surface(&self, id: SurfaceId) -> SurfaceHandle<'_> {
        SurfaceHandle {
            surface_id: id,
            ctx: self,
        }
    }

    pub fn publish(&self, topic: &str, payload: &impl Serialize) -> Result<(), PluginError> {
        let value = serde_json::to_value(payload).map_err(|e| PluginError::Serialization(e.to_string()))?;
        self.api.publish(topic, value)
    }

    pub fn deactivate_self(&self) -> Result<(), PluginError> {
        Ok(())
    }
}

pub struct UpgradeContext<'a> {
    pub store: &'a mut Store,
    pub config: &'a crate::manifest::Manifest,
}

pub trait PluginApi: Send + Sync {
    fn dispatch(&self, cmd: CommandInvocation) -> Result<(), PluginError>;
    fn request(&self, cmd: CommandInvocation) -> Result<serde_json::Value, PluginError>;
    fn publish(&self, topic: &str, payload: serde_json::Value) -> Result<(), PluginError>;
    fn render(&self, surface_id: SurfaceId, frame: &crate::render::Frame) -> Result<(), PluginError>;
    fn store_read(&self, key: &str) -> Result<Option<bytes::Bytes>, PluginError>;
    fn store_write(&self, key: &str, value: bytes::Bytes) -> Result<(), PluginError>;
    fn store_delete(&self, key: &str) -> Result<(), PluginError>;
    fn store_list(&self, prefix: &str) -> Result<Vec<String>, PluginError>;
}
