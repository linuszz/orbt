pub mod wasm;
pub mod native;
pub mod thread_pool;

pub use wasm::WasmRuntime;
pub use native::NativeRuntime;
pub use thread_pool::NativeThreadPool;

pub const PLUGIN_ABI_VERSION: u32 = 1;
