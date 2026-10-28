#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! TubeGlass: a desktop YouTube client (videos, music and podcasts) written in Rust.
//!
//! The code is organised in layers:
//!
//! * [`app`]      – the application state and its logic (playing, queues, downloads, navigation).
//! * [`ui`]       – everything that draws: pages, widgets, theme, icons.
//! * [`data`]     – settings, files on disk and the types shared across the app.
//! * [`youtube`]  – talking to YouTube and YouTube Music (no account, no API key).
//! * [`playback`] – the ffmpeg-based player, the local range proxy, MPRIS media keys.
//! * [`library`]  – downloads, clips, podcasts, subscriptions import and statistics.
//! * [`platform`] – the few things that differ between Linux, macOS and Windows.
//! * [`util`]     – small helpers.

mod app;
mod data;
mod library;
mod locale_format;
mod platform;
mod playback;
mod ui;
mod util;
mod youtube;

#[cfg(test)]
mod tests;

// The names below are shared crate-wide (`use crate::*;` in every file), so a module can use
// `Settings`, `accent()` or `yt::Video` without spelling out where each one lives.
#[allow(unused_imports)]
pub(crate) use {app::*, data::*, library::*, playback::*, ui::*, util::*, youtube::*};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, vec2,
};
use player::{Player, State};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use download::DlItem;
use yt::{Channel, ChannelAbout, ChannelHeader, ChannelHit, Comment, Kind, PlaylistHit, Streams, TabData, Video};

/// Opens the window and runs the app until it is closed.
fn main() -> eframe::Result<()> {
    platform::prepare_environment();
    platform::install_crash_log();
    let ws = load_win();
    let size = if ws.w >= 820.0 && ws.h >= 520.0 { [ws.w, ws.h] } else { [1360.0, 840.0] };
    let mut vb = egui::ViewportBuilder::default();
    if let (Some(x), Some(y)) = (ws.x, ws.y) {
        // ignore positions that would put the window out of reach
        if x > -4000.0 && y > -2000.0 && x < 12000.0 && y < 8000.0 {
            vb = vb.with_position(egui::pos2(x, y));
        }
    }
    let opts = eframe::NativeOptions {
        viewport: vb
            .with_title("TubeGlass")
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: icons::render_app_icon(256),
                width: 256,
                height: 256,
            }))
            .with_inner_size(size)
            .with_maximized(ws.maximized)
            .with_min_inner_size([820.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native("TubeGlass", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
