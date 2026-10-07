use crate::error::PluginError;
use crate::plugin::Plugin;
use crate::types::PluginId;

pub struct NativeRuntime {
    _phantom: std::marker::PhantomData<()>,
}

impl NativeRuntime {
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn load_plugin(
        &self,
        _plugin_id: PluginId,
        _lib_path: &std::path::Path,
    ) -> Result<Box<dyn Plugin>, PluginError> {
        #[cfg(feature = "native")]
        {
            unimplemented!("Native runtime requires libloading")
        }
        #[cfg(not(feature = "native"))]
        {
            Err(PluginError::Panicked(
                "Native runtime not enabled (feature = \"native\")".to_string(),
            ))
        }
    }
}
