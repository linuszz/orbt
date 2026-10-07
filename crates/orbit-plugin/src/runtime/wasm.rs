use crate::error::PluginError;
use crate::plugin::Plugin;
use crate::types::PluginId;

pub struct WasmRuntime {
    _phantom: std::marker::PhantomData<()>,
}

impl WasmRuntime {
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn load_plugin(
        &self,
        _plugin_id: PluginId,
        _wasm_bytes: &[u8],
    ) -> Result<Box<dyn Plugin>, PluginError> {
        #[cfg(feature = "wasm")]
        {
            unimplemented!("WASM runtime requires wasmtime dependency")
        }
        #[cfg(not(feature = "wasm"))]
        {
            Err(PluginError::WasmTrap(
                "WASM runtime not enabled (feature = \"wasm\")".to_string(),
            ))
        }
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}
