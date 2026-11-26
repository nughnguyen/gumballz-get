use gumballz_plugin_sdk::{GumballzPlugin, PluginHost};
use std::{future::Future, pin::Pin, sync::Arc};

pub struct ExamplePlugin(Option<Arc<dyn PluginHost>>);

impl ExamplePlugin {
    pub fn new() -> Self { Self(None) }
}

impl GumballzPlugin for ExamplePlugin {
    fn id(&self) -> &str { "example-plugin" }
    fn name(&self) -> &str { "Example Plugin" }
    fn version(&self) -> &str { env!("CARGO_PKG_VERSION") }
    fn initialize(&mut self, host: Arc<dyn PluginHost>) -> anyhow::Result<()> { self.0 = Some(host); Ok(()) }
    fn handle_command(&self, command: String, _args: serde_json::Value) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + 'static>> {
        Box::pin(async move { Err(format!("Unknown command: {command}")) })
    }
    fn commands(&self) -> Vec<String> { Vec::new() }
}

gumballz_plugin_sdk::export_plugin!(ExamplePlugin::new());
