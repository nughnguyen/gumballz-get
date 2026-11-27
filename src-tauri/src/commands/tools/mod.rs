//! Tauri commands for the Tools section. Each submodule is a UI group (voice,
//! AI, YouTube, documents, images, files, downloads, system, phone); the logic
//! lives in `gumballz_core::core::tools`. Progress comes out through the
//! `tool-progress` event with the `ToolProgress` payload.

pub mod ai;
pub mod audio;
pub mod bilibili;
pub mod blogs;
pub mod ctf;
pub mod desktop;
pub mod documents;
pub mod downloads;
pub mod files;
pub mod games;
pub mod images;
pub mod instagram;
pub mod linkedin;
pub mod lists;
pub mod music;
pub mod pdf;
pub mod phone;
pub mod pinterest;
pub mod reddit;
pub mod speech;
pub mod system;
pub mod text;
pub mod tiktok;
pub mod tumblr;
pub mod twitch;
pub mod video;
pub mod vimeo;
pub mod x;
pub mod youtube;

use std::sync::Arc;

use gumballz_core::core::tools::ProgressFn;
use tauri::Emitter;

pub(crate) fn progress(app: &tauri::AppHandle) -> ProgressFn {
    let app = app.clone();
    Arc::new(move |p| {
        let _ = app.emit("tool-progress", p);
    })
}

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
