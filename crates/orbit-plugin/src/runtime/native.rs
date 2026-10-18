use crate::error::PluginError;
use crate::plugin::Plugin;
use crate::types::PluginId;
use super::PLUGIN_ABI_VERSION;

#[cfg(feature = "native")]
pub struct NativeRuntime {
    loaded_libs: Vec<libloading::Library>,
}

#[cfg(feature = "native")]
impl NativeRuntime {
    pub fn new() -> Self {
        Self {
            loaded_libs: Vec::new(),
        }
    }

    pub fn load_plugin(
        &mut self,
        _plugin_id: PluginId,
        lib_path: &std::path::Path,
    ) -> Result<Box<dyn Plugin>, PluginError> {
        let lib = unsafe { libloading::Library::new(lib_path) }
            .map_err(|e| PluginError::Panicked(format!("dlopen failed: {}", e)))?;

        let abi_fn: libloading::Symbol<unsafe extern "C" fn() -> u32> = unsafe {
            lib.get(b"orbit_plugin_abi_version\0")
        }
        .map_err(|_| PluginError::Panicked("missing orbit_plugin_abi_version".into()))?;

        let abi = unsafe { abi_fn() };
        if abi != PLUGIN_ABI_VERSION {
            return Err(PluginError::Panicked(format!(
                "ABI version mismatch: plugin v{}, host v{}",
                abi, PLUGIN_ABI_VERSION
            )));
        }

        let create_fn: libloading::Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> = unsafe {
            lib.get(b"orbit_plugin_create\0")
        }
        .map_err(|_| PluginError::Panicked("missing orbit_plugin_create".into()))?;

        let plugin_ptr = unsafe { create_fn() };
        if plugin_ptr.is_null() {
            return Err(PluginError::Panicked("orbit_plugin_create returned null".into()));
        }

        let plugin = unsafe { Box::from_raw(plugin_ptr) };
        self.loaded_libs.push(lib);
        Ok(plugin)
    }

    pub fn loaded_count(&self) -> usize {
        self.loaded_libs.len()
    }
}

#[cfg(not(feature = "native"))]
pub struct NativeRuntime {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(not(feature = "native"))]
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
        Err(PluginError::Panicked(format!(
            "Native runtime not enabled (feature = \"native\"): {:?}",
            lib_path
        )))
    }
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}
