#[macro_export]
macro_rules! export_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn orbit_plugin_abi_version() -> u32 {
            $crate::runtime::PLUGIN_ABI_VERSION
        }

        #[no_mangle]
        pub extern "C" fn orbit_plugin_create() -> *mut dyn $crate::plugin::Plugin {
            let plugin: $plugin_type = <$plugin_type as $crate::plugin::Plugin>::new();
            Box::into_raw(Box::new(plugin))
        }
    };
}
