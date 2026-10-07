use crate::error::LoadError;
use crate::manifest::Dependency;
use crate::types::PluginId;

pub struct DependencyResolver;

impl DependencyResolver {
    pub fn resolve(
        _plugin_id: &PluginId,
        _deps: &[Dependency],
    ) -> Result<Vec<PluginId>, LoadError> {
        Ok(Vec::new())
    }
}
