use std::sync::Arc;

use crate::host::PluginHost;

pub trait GumballzPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn initialize(&mut self, host: Arc<dyn PluginHost>) -> anyhow::Result<()>;
    fn shutdown(&self) {}

    fn handle_command(
        &self,
        command: String,
        args: serde_json::Value,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send + 'static>,
    >;

    fn commands(&self) -> Vec<String>;
}

/// Emit the three `#[no_mangle]` C symbols the dynamic loader looks up
/// (`gumballz_get_plugin_abi_version`, `gumballz_get_plugin_build_info`,
/// `gumballz_get_plugin_init`).
///
/// The symbols are gated on the calling crate's `dynamic` feature, so an
/// rlib built without it emits none of them. This matters because the app
/// crate links all five plugin crates as rlibs: if every rlib defined the
/// same three global symbols, the Windows link of the app failed with
/// LNK2005 (already defined) / LNK1169. A standalone cdylib build enables
/// the feature (`cargo build -p <plugin> --features dynamic`) and still
/// exports all three symbols, which is what the loader requires.
///
/// `#[macro_export]` macros expand in the caller crate, so `cfg(feature =
/// "dynamic")` below is evaluated against the *caller's* features, not
/// the SDK's — each plugin crate defines its own (non-default) `dynamic`
/// feature, and the app depends on the plugins with
/// `default-features = false` and without `dynamic`.
#[macro_export]
macro_rules! export_plugin {
    ($constructor:expr) => {
        #[cfg(feature = "dynamic")]
        #[no_mangle]
        pub extern "C" fn gumballz_get_plugin_abi_version() -> u32 {
            $crate::ABI_VERSION
        }

        #[cfg(feature = "dynamic")]
        #[no_mangle]
        pub extern "C" fn gumballz_get_plugin_build_info() -> *const ::std::os::raw::c_char {
            $crate::BUILD_INFO.as_ptr() as *const ::std::os::raw::c_char
        }

        #[cfg(feature = "dynamic")]
        #[no_mangle]
        pub extern "C" fn gumballz_get_plugin_init() -> *mut dyn $crate::GumballzPlugin {
            let plugin = $constructor;
            Box::into_raw(Box::new(plugin))
        }
    };
}
