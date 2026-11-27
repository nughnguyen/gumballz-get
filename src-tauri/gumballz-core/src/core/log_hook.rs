use std::sync::{Arc, OnceLock};

pub type LogSink = Arc<dyn Fn(u64, &str) + Send + Sync + 'static>;

static SINK: OnceLock<LogSink> = OnceLock::new();

pub fn set_log_sink(sink: LogSink) {
    let _ = SINK.set(sink);
}

pub fn emit_log(id: u64, line: &str) {
    if let Some(s) = SINK.get() {
        s(id, line);
    }
}

tokio::task_local! {
    pub static CURRENT_DOWNLOAD_ID: u64;
    pub static CURRENT_COOKIE_SLUG: Option<String>;
    /// Full yt-dlp command typed by the user ("edit and retry").
    /// When present, `download_video` runs these tokens as-is,
    /// only adding the progress instrumentation, and makes a single
    /// attempt. Task-local for the same reason as the id: it avoids passing one more
    /// parameter through ten platforms.
    pub static CURRENT_ARGV_OVERRIDE: Option<Vec<String>>;
}

pub fn current_download_id() -> Option<u64> {
    CURRENT_DOWNLOAD_ID.try_with(|v| *v).ok()
}

pub fn current_cookie_slug() -> Option<String> {
    CURRENT_COOKIE_SLUG.try_with(|v| v.clone()).ok().flatten()
}

pub fn current_argv_override() -> Option<Vec<String>> {
    CURRENT_ARGV_OVERRIDE.try_with(|v| v.clone()).ok().flatten()
}
