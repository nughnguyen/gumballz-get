/// A Tokio runtime owned by the plugin crate itself.
///
/// A dynamically loaded plugin (.dll/.so) links its own copy of tokio,
/// separate from the host app's copy, so the host runtime's handle is
/// invisible to tokio calls made from plugin code: the first `tokio::spawn`
/// or `tokio::time::timeout` panics with "there is no reactor running".
/// Every plugin command must therefore run on a runtime built inside the
/// plugin crate. This struct owns that runtime: keep one per plugin and
/// run every command through [`PluginRuntime::run`]. Build it lazily on the
/// first command, not in `initialize`, so direct callers (unit tests) that
/// construct the plugin with `new()` and call `handle_command` without
/// `initialize` still get a runtime.
pub struct PluginRuntime {
    // Deliberately leaked: dropping a multi-thread Runtime blocks the
    // current thread to shut it down, and Tokio panics when that happens
    // from within an asynchronous context ("Cannot drop a runtime in a
    // context where blocking is not allowed") — which is exactly where a
    // plugin is dropped in unit tests. The leak matches the real lifetime
    // anyway: the loader never unloads a plugin, so its runtime lives
    // until the process exits.
    runtime: &'static tokio::runtime::Runtime,
}

impl PluginRuntime {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            runtime: Box::leak(Box::new(tokio::runtime::Runtime::new()?)),
        })
    }

    /// Run `fut` on this plugin's own runtime and return its output.
    /// A panic inside the task is caught by the runtime and reported as
    /// an error instead of aborting the process.
    pub async fn run<F, T>(&self, fut: F) -> Result<T, String>
    where
        F: std::future::Future<Output = Result<T, String>> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime
            .spawn(fut)
            .await
            .map_err(|e| format!("Plugin task failed: {e}"))?
    }
}
