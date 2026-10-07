use crate::error::PluginError;
use crate::plugin::Plugin;
use crate::types::PluginId;
use super::PLUGIN_ABI_VERSION;

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
        &mut self,
        _plugin_id: PluginId,
        lib_path: &std::path::Path,
    ) -> Result<Box<dyn Plugin>, PluginError> {
        #[cfg(feature = "native")]
        {
            unimplemented!("Native runtime requires libloading dependency")
        }
        #[cfg(not(feature = "native"))]
        {
            Err(PluginError::Panicked(format!(
                "Native runtime not enabled (feature = \"native\"): {:?}",
                lib_path
            )))
        }
    }
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}
