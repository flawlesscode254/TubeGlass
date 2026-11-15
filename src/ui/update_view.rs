//! The "new version available" bar shown above the pages.

use crate::*;
use crate::update;

impl App {
    /// The bar about ffmpeg: setting it up (with progress) when it was missing at launch, or
    /// explaining how to install it when the download did not work.
    pub(crate) fn ffmpeg_bar(&mut self, ui: &mut egui::Ui) {
        let state = self.ffmpeg.clone();
        if state == FfmpegState::Ready || (matches!(state, FfmpegState::Failed(_)) && self.ffmpeg_hidden) {
            return;
        }
        let (mut retry, mut open, mut hide) = (false, false, false);
        let fill = match state {
            FfmpegState::Failed(_) => LIVE.gamma_multiply(0.28),
            _ => accent().gamma_multiply(0.22),
        };
        egui::Panel::top("ffmpeg-bar")
            .exact_size(44.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(24, 6)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| match &state {
                    FfmpegState::Downloading(p) => {
                        ui.label(egui::RichText::new(tr("Setting up ffmpeg (once, about 30 MB) so video and audio can play…")).strong().size(14.0));
                        progress_pill(ui, *p, &format!("{}%", (p * 100.0) as u32), 180.0);
                    }
                    FfmpegState::Failed(e) => {
                        ui.label(egui::RichText::new(tr("ffmpeg is not available, so nothing can play yet.")).strong().size(14.0));
                        // the explanation gives way (with an ellipsis) so the buttons always stay visible
                        let room = (ui.available_width() - 330.0).max(80.0);
                        let msg = format!("{e}  {}", crate::platform::ffmpeg_hint());
                        ui.allocate_ui(vec2(room, 24.0), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(msg).size(13.0).color(muted())).truncate());
                        });
                        if icon_btn(ui, "refresh", &tr("Try again"), Some(accent()), ON_ACCENT, false, true).clicked() {
                            retry = true;
                        }
                        if icon_btn(ui, "", &tr("Get ffmpeg"), None, ink(), false, true).clicked() {
                            open = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_btn(ui, "close", "", None, ink(), false, true).on_hover_text(tr("Hide")).clicked() {
                                hide = true;
                            }
                        });
                    }
                    FfmpegState::Ready => {}
                });
            });
        if retry {
            self.start_ffmpeg_setup();
        }
        if open {
            crate::platform::open_url("https://ffmpeg.org/download.html");
        }
        if hide {
            self.ffmpeg_hidden = true;
        }
    }

    /// Draws the update bar (nothing when there is no update). Returns its height so callers
    /// can place other things around it; the bar itself is a top panel.
    pub(crate) fn update_bar(&mut self, ui: &mut egui::Ui) {
        let Some(rel) = self.update.clone() else { return };
        let mut start = false;
        let mut skip = false;
        let mut dismiss = false;
        let mut notes = false;
        let mut restart = false;
        egui::Panel::top("update-bar")
            .exact_size(44.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(accent().gamma_multiply(0.22)).inner_margin(egui::Margin::symmetric(24, 6)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    icons::draw(ui.painter(), ui.cursor().min + vec2(10.0, 16.0), "download", 17.0, accent());
                    ui.add_space(26.0);
                    match self.update_state.clone() {
                        UpdateState::Idle | UpdateState::Failed(_) => {
                            ui.label(egui::RichText::new(trf("TubeGlass {version} is available", &[("version", &rel.version)])).strong().size(14.5));
                            ui.label(egui::RichText::new(trf("(you have {version})", &[("version", update::CURRENT)])).size(13.0).color(muted()));
                            if let UpdateState::Failed(e) = &self.update_state {
                                ui.colored_label(LIVE, truncate(ui, e, FontId::proportional(13.0), 360.0, 1));
                            }
                            let have_file = update::pick_asset(&rel.assets, update::detect_install(), std::env::consts::ARCH).is_some();
                            if icon_btn(ui, "download", &tr(if have_file { "Update" } else { "Open release page" }), Some(accent()), ON_ACCENT, false, true).clicked() {
                                start = true;
                            }
                            if !rel.notes.is_empty() && icon_btn(ui, "",&tr("What's new"), None, ink(), false, true).clicked() {
                                notes = true;
                            }
                            if icon_btn(ui, "",&tr("Skip this version"), None, ink(), false, true).clicked() {
                                skip = true;
                            }
                        }
                        UpdateState::Downloading(p) => {
                            ui.label(egui::RichText::new(trf("Downloading TubeGlass {version}…", &[("version", &rel.version)])).size(14.0));
                            progress_pill(ui, p, &format!("{}%", (p * 100.0) as u32), 200.0);
                        }
                        UpdateState::Done(path, installed) => {
                            if installed {
                                ui.label(egui::RichText::new(trf("TubeGlass {version} is installed. Restart to use it.", &[("version", &rel.version)])).strong().size(14.5));
                                if icon_btn(ui, "refresh",&tr("Restart now"), Some(accent()), ON_ACCENT, false, true).clicked() {
                                    restart = true;
                                }
                            } else {
                                ui.label(egui::RichText::new(trf("Saved to {path}", &[("path", &path.display().to_string())])).size(13.5));
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Hide for now"));
                        if resp.hovered() {
                            ui.painter().circle_filled(r.center(), 14.0, ov(30));
                        }
                        icons::draw(ui.painter(), r.center(), "close", 13.0, ink());
                        if resp.clicked() {
                            dismiss = true;
                        }
                    });
                });
            });
        if start {
            self.start_update();
        }
        if skip {
            self.skip_update();
        }
        if dismiss {
            // hidden until the next check finds it again
            self.update = None;
        }
        if notes {
            self.update_notes_open = !self.update_notes_open;
        }
        if restart {
            crate::platform::restart();
        }
        if self.update_notes_open {
            let mut open = true;
            egui::Window::new(trf("What's new in {version}", &[("version", &rel.version)]))
                .open(&mut open)
                .default_width(460.0)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        ui.label(egui::RichText::new(&rel.notes).size(13.5));
                    });
                    if !rel.page.is_empty() && ui.link(tr("Open the release page")).clicked() {
                        crate::platform::open_url(&rel.page);
                    }
                });
            self.update_notes_open = open;
        }
    }
}
