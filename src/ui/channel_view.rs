//! The channel page header, tabs and About section.

use crate::*;

impl App {
    /// True while the Playlists tab of a channel is open (it shows playlists, not videos).
    pub(crate) fn sel_is_chan_playlists(&self) -> bool {
        matches!(self.sel, Sel::Channel(_)) && self.chan_tab == ChanTab::Playlists
    }

    /// Draws the channel's banner, avatar, actions, tabs and search box.
    pub(crate) fn channel_header(&mut self, ui: &mut egui::Ui, busy: bool) {
        let Sel::Channel(id) = self.sel.clone() else { return };
        let Some(ch) = self.channels.iter().find(|c| c.id == id).cloned() else { return };
        let hdr = self.fcache.headers.get(&id).cloned().unwrap_or_default();
        let w = ui.available_width();

        // banner
        if !hdr.banner.is_empty() {
            let bh = (w / 5.5).clamp(80.0, 170.0);
            let (r, _) = ui.allocate_exact_size(vec2(w, bh), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(16), card());
            let uri = img_uri(ui.ctx(), &hdr.banner);
            if !uri.is_empty() {
                egui::Image::new(uri)
                    .fit_to_exact_size(r.size())
                    .maintain_aspect_ratio(false)
                    .corner_radius(CornerRadius::same(16))
                    .paint_at(ui, r);
            }
            ui.add_space(8.0);
        }

        // refresh / play all / "updated": next to the name on a wide window, under it on a narrow one
        let narrow_hdr = w < 920.0;
        let actions = |app: &mut App, ui: &mut egui::Ui| {
            if icon_btn(ui, "refresh", &tr("Refresh"), None, ink(), false, true).clicked() {
                app.refresh(&ch);
                app.ensure_tab(true);
            }
            if matches!(app.chan_tab, ChanTab::Videos | ChanTab::Live | ChanTab::Shorts) {
                if icon_btn(ui, "play", &tr("Play all"), Some(accent()), ON_ACCENT, false, true).clicked() {
                    app.play_all_visible(false);
                }
            }
            // the first press asks, the second one unsubscribes
            let sure = app.confirm_unsub;
            let label = tr(if sure { "Confirm unsubscribe" } else { "Unsubscribe" });
            if icon_btn(ui, "", &label, if sure { Some(LIVE) } else { None }, if sure { Color32::WHITE } else { ink() }, false, true).clicked() {
                if sure {
                    app.confirm_unsub = false;
                    app.remove_channel(&ch.id);
                    app.toast(trf("Unsubscribed from {channel}", &[("channel", &ch.name)]));
                } else {
                    app.confirm_unsub = true;
                }
            }
            if let Some(ts) = app.fcache.updated.get(&ch.id) {
                ui.label(egui::RichText::new(trf("Updated {time}", &[("time", &ago(*ts))])).size(13.0).color(muted()));
            }
        };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(76.0, 76.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 38.0, card_hi());
            let uri = img_uri(ui.ctx(), &ch.avatar);
            if !uri.is_empty() {
                egui::Image::new(uri).corner_radius(CornerRadius::same(38)).paint_at(ui, r);
            }
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&ch.name).size(28.0).strong());
                    if busy {
                        ui.add(egui::Spinner::new().size(18.0));
                    }
                });
                let meta = [hdr.handle.as_str(), hdr.subs.as_str(), hdr.videos.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("  •  ");
                if !meta.is_empty() {
                    ui.label(egui::RichText::new(meta).size(14.0).color(muted()));
                }
                if !hdr.description.is_empty() {
                    let d = truncate(ui, &hdr.description, FontId::proportional(14.0), (w - 380.0).max(240.0), 1);
                    ui.label(egui::RichText::new(d).size(14.0).color(muted()));
                }
            });
            if !narrow_hdr {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| actions(self, ui));
            }
        });
        if narrow_hdr {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| actions(self, ui));
        }
        ui.add_space(10.0);

        // tabs
        ui.horizontal_wrapped(|ui| {
            for (t, label) in [
                (ChanTab::Videos, tr("Videos")),
                (ChanTab::Live, tr("Live")),
                (ChanTab::Shorts, tr("Shorts")),
                (ChanTab::Playlists, tr("Playlists")),
                (ChanTab::About, tr("About")),
            ] {
                let on = self.chan_tab == t;
                let g = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), FontId::proportional(16.0), ink()));
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 28.0, 36.0), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                if on {
                    ui.painter().add(surface::filled(r, r.height() / 2.0, accent()));
                } else if hv > 0.0 {
                    ui.painter().add(surface::capsule(r, 0.6 * hv, None, false));
                }
                ui.painter().text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(16.0), if on { ON_ACCENT } else { ink().gamma_multiply(0.72) });
                if resp.clicked() && !on {
                    self.chan_tab = t;
                }
            }
            // the search box sits beside the tabs when there is room, under them otherwise
            if w >= 820.0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let width = 330.0_f32.min(ui.available_width());
                    let hint = trf("Search in {channel}", &[("channel", &ch.name)]);
                    let f = text_field(ui, &mut self.chan_q, &hint, width, 40.0, "search", None, false);
                    if f.submit {
                        self.start_chan_search();
                    }
                    if f.cleared {
                        self.chan_search = None;
                    }
                });
            }
        });
        if w < 820.0 {
            ui.add_space(6.0);
            let hint = trf("Search in {channel}", &[("channel", &ch.name)]);
            let f = text_field(ui, &mut self.chan_q, &hint, w, 40.0, "search", None, false);
            if f.submit {
                self.start_chan_search();
            }
            if f.cleared {
                self.chan_search = None;
            }
        }
        // a different channel starts with a clean search box
        if self.chan_q_for != id {
            self.chan_q_for = id.clone();
            self.chan_q.clear();
            self.chan_search = None;
        }
        if let Some(cs) = self.active_chan_search() {
            let (n_v, n_p) = cs
                .res
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(|(v, p)| (v.len(), p.len()))
                .unwrap_or((0, 0));
            let q = cs.query.clone();
            let loading = cs.res.is_none();
            let failed = cs.res.as_ref().and_then(|r| r.as_ref().err()).cloned();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let msg = if loading {
                    trf("Searching “{query}”…", &[("query", &q)])
                } else if let Some(e) = &failed {
                    trf("Search failed: {error}", &[("error", e)])
                } else {
                    trf("Results for “{query}”  •  {videos} videos, {playlists} playlists", &[("query", &q), ("videos", &n_v.to_string()), ("playlists", &n_p.to_string())])
                };
                ui.label(egui::RichText::new(msg).size(13.0).color(if failed.is_some() { LIVE } else { muted() }));
            });
        }
    }

    /// Draws the About tab: description, stats and links.
    pub(crate) fn about_ui(&mut self, ui: &mut egui::Ui, about: Option<ChannelAbout>) {
        let Some(a) = about else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(26.0).color(accent()));
            });
            return;
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let card = |ui: &mut egui::Ui, f: &mut dyn FnMut(&mut egui::Ui)| {
                egui::Frame::new()
                    .fill(card())
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        f(ui);
                    });
                ui.add_space(12.0);
            };
            if !a.description.is_empty() {
                card(ui, &mut |ui| {
                    ui.label(egui::RichText::new(tr("Description")).size(13.0).color(muted()).strong());
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(egui::RichText::new(&a.description).size(15.0)).wrap());
                });
            }
            card(ui, &mut |ui| {
                ui.label(egui::RichText::new(tr("Stats")).size(13.0).color(muted()).strong());
                ui.add_space(4.0);
                for (k, v) in [
                    (tr("Subscribers"), &a.subs),
                    (tr("Videos"), &a.videos),
                    (tr("Views"), &a.views),
                    (tr("Joined"), &a.joined),
                    (tr("Country"), &a.country),
                ] {
                    if !v.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new(k).color(muted())));
                            ui.label(egui::RichText::new(v.as_str()).size(15.0));
                        });
                    }
                }
            });
            if !a.links.is_empty() {
                card(ui, &mut |ui| {
                    ui.label(egui::RichText::new(tr("Links")).size(13.0).color(muted()).strong());
                    ui.add_space(4.0);
                    for (title, url) in &a.links {
                        ui.horizontal(|ui| {
                            ui.add_sized([110.0, 20.0], egui::Label::new(egui::RichText::new(title).color(muted())));
                            ui.hyperlink_to(url.trim_start_matches("https://"), url);
                        });
                    }
                });
            }
        });
    }
}
