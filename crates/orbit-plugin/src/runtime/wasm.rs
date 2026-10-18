use crate::error::PluginError;
use crate::plugin::Plugin;
use crate::types::PluginId;

#[cfg(feature = "wasm")]
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

#[cfg(feature = "wasm")]
pub struct WasmRuntime {
    engine: Engine,
}

#[cfg(feature = "wasm")]
pub struct WasmInstance {
    store: Store<HostState>,
    instance: Instance,
    plugin_id: PluginId,
    alloc_fn: TypedFunc<u32, u32>,
}

#[cfg(feature = "wasm")]
pub struct HostState {
    limits: StoreLimits,
    plugin_id: PluginId,
}

#[cfg(feature = "wasm")]
impl WasmRuntime {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.wasm_threads(false);
        config.wasm_simd(true);
        config.wasm_bulk_memory(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).expect("failed to create wasmtime engine");
        Self { engine }
    }

    pub fn load_plugin(
        &self,
        plugin_id: PluginId,
        wasm_bytes: &[u8],
    ) -> Result<WasmInstance, PluginError> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| PluginError::WasmTrap(format!("module compile failed: {}", e)))?;

        let limits = StoreLimitsBuilder::new()
            .memory_size(256 * 1024 * 1024)
            .build();

        let mut store = Store::new(
            &self.engine,
            HostState {
                limits,
                plugin_id: plugin_id.clone(),
            },
        );
        store.limiter(|state| &mut state.limits);
        store.set_fuel(1_000_000).map_err(|e| {
            PluginError::WasmTrap(format!("failed to set fuel: {}", e))
        })?;

        let mut linker = Linker::new(&self.engine);

        linker
            .func_wrap(
                "orbit",
                "orbit_dispatch",
                |mut caller: Caller<'_, HostState>, cmd_ptr: u32, cmd_len: u32| -> u32 {
                    let memory = match caller.get_export("memory") {
                        Some(wasmtime::Extern::Memory(m)) => m,
                        _ => return 0,
                    };
                    let data = memory
                        .data(&caller)
                        .get(cmd_ptr as usize..(cmd_ptr + cmd_len) as usize)
                        .unwrap_or(&[]);
                    let _ = data;
                    0
                },
            )
            .map_err(|e| PluginError::WasmTrap(format!("linker error: {}", e)))?;

        linker
            .func_wrap(
                "orbit",
                "orbit_publish",
                |mut caller: Caller<'_, HostState>,
                 topic_ptr: u32,
                 topic_len: u32,
                 payload_ptr: u32,
                 payload_len: u32| {
                    let memory = match caller.get_export("memory") {
                        Some(wasmtime::Extern::Memory(m)) => m,
                        _ => return,
                    };
                    let _topic = memory
                        .data(&caller)
                        .get(topic_ptr as usize..(topic_ptr + topic_len) as usize)
                        .unwrap_or(&[]);
                    let _payload = memory
                        .data(&caller)
                        .get(payload_ptr as usize..(payload_ptr + payload_len) as usize)
                        .unwrap_or(&[]);
                },
            )
            .map_err(|e| PluginError::WasmTrap(format!("linker error: {}", e)))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| PluginError::WasmTrap(format!("instantiate failed: {}", e)))?;

        let alloc_fn = instance
            .get_typed_func::<u32, u32>(&mut store, "orbit_plugin_alloc")
            .map_err(|e| PluginError::WasmTrap(format!("missing orbit_plugin_alloc: {}", e)))?;

        Ok(WasmInstance {
            store,
            instance,
            plugin_id,
            alloc_fn,
        })
    }
}

#[cfg(feature = "wasm")]
impl WasmInstance {
    pub fn call_init(&mut self, ctx_data: &[u8]) -> Result<(), PluginError> {
        let init_fn = self
            .instance
            .get_typed_func::<(u32, u32), u32>(&mut self.store, "orbit_plugin_init")
            .map_err(|e| PluginError::WasmTrap(format!("missing orbit_plugin_init: {}", e)))?;

        let ptr = self.alloc_guest(ctx_data.len() as u32)?;
        self.write_guest(ptr, ctx_data)?;

        let result_ptr = init_fn
            .call(&mut self.store, (ptr, ctx_data.len() as u32))
            .map_err(|e| PluginError::WasmTrap(format!("init call failed: {}", e)))?;

        let _result = self.read_guest(result_ptr)?;
        Ok(())
    }

    pub fn call_on_activate(&mut self) -> Result<(), PluginError> {
        let func = self
            .instance
            .get_typed_func::<u32, u32>(&mut self.store, "orbit_plugin_on_activate")
            .map_err(|e| PluginError::WasmTrap(format!("missing on_activate: {}", e)))?;
        let result = func
            .call(&mut self.store, 0)
            .map_err(|e| PluginError::WasmTrap(format!("on_activate failed: {}", e)))?;
        let _ = result;
        Ok(())
    }

    pub fn call_on_deactivate(&mut self) -> Result<(), PluginError> {
        let func = self
            .instance
            .get_typed_func::<u32, u32>(&mut self.store, "orbit_plugin_on_deactivate")
            .map_err(|e| PluginError::WasmTrap(format!("missing on_deactivate: {}", e)))?;
        let result = func
            .call(&mut self.store, 0)
            .map_err(|e| PluginError::WasmTrap(format!("on_deactivate failed: {}", e)))?;
        let _ = result;
        Ok(())
    }

    pub fn call_shutdown(&mut self) -> Result<(), PluginError> {
        let func = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "orbit_plugin_shutdown")
            .map_err(|e| PluginError::WasmTrap(format!("missing shutdown: {}", e)))?;
        func.call(&mut self.store, ())
            .map_err(|e| PluginError::WasmTrap(format!("shutdown failed: {}", e)))?;
        Ok(())
    }

    fn alloc_guest(&mut self, size: u32) -> Result<u32, PluginError> {
        self.alloc_fn
            .call(&mut self.store, size)
            .map_err(|e| PluginError::WasmTrap(format!("alloc failed: {}", e)))
    }

    fn write_guest(&mut self, ptr: u32, data: &[u8]) -> Result<(), PluginError> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| PluginError::WasmTrap("no memory export".into()))?;
        memory
            .write(&mut self.store, ptr as usize, data)
            .map_err(|e| PluginError::WasmTrap(format!("memory write failed: {}", e)))
    }

    fn read_guest(&mut self, ptr: u32) -> Result<Vec<u8>, PluginError> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| PluginError::WasmTrap("no memory export".into()))?;
        let data_size = 1024;
        let mut buf = vec![0u8; data_size];
        memory
            .read(&mut self.store, ptr as usize, &mut buf)
            .map_err(|e| PluginError::WasmTrap(format!("memory read failed: {}", e)))?;
        Ok(buf)
    }
}

#[cfg(not(feature = "wasm"))]
pub struct WasmRuntime {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(not(feature = "wasm"))]
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
        Err(PluginError::WasmTrap(
            "WASM runtime not enabled (feature = \"wasm\")".to_string(),
        ))
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}
