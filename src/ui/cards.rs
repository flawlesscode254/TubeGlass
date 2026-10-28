//! The video card shown in grids and lists, with its right-click menu.

use crate::*;

/// Returns true when clicked.
pub(crate) fn video_card(ui: &mut egui::Ui, rect: Rect, th: f32, v: &Video, info: &CardInfo) -> (bool, bool, Option<CardAction>) {
    let id = ui.scope_id().with(&v.id);
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = ui.ctx().animate_bool_with_time(id.with("hover"), resp.hovered(), 0.15);
    let r = rect.translate(vec2(0.0, -3.0 * h));
    let p = ui.painter();
    p.add(surface::panel(
        r.expand(h * 2.0),
        22.0,
        0.8 + 0.2 * h,
        if h > 0.0 { Some(accent().gamma_multiply(0.35 * h)) } else { None },
        true,
    ));
    let img = if info.row {
        let hh = r.height() - 14.0;
        Rect::from_min_size(r.min + vec2(7.0, 7.0), vec2(hh * 16.0 / 9.0, hh))
    } else {
        Rect::from_min_size(r.min + vec2(7.0, 7.0), vec2(r.width() - 14.0, th - 7.0))
    };
    p.rect_filled(img, CornerRadius::same(16), ov(14));
    let uri = img_uri(ui.ctx(), &v.thumb);
    if !uri.is_empty() {
        egui::Image::new(uri)
            .fit_to_exact_size(img.size())
            .corner_radius(CornerRadius::same(12))
            .paint_at(ui, img);
    }

    let p = ui.painter();
    // badges
    match v.kind {
        Kind::Live => {
            let b = Rect::from_min_size(img.min + vec2(8.0, 8.0), vec2(62.0, 22.0));
            p.rect_filled(b, CornerRadius::same(6), LIVE);
            let pulse = 0.55 + 0.45 * (ui.input(|i| i.time) * 3.0).sin().abs() as f32;
            p.circle_filled(b.left_center() + vec2(12.0, 0.0), 4.0, Color32::WHITE.gamma_multiply(pulse));
            p.text(b.center() + vec2(7.0, 0.0), Align2::CENTER_CENTER, "LIVE", FontId::proportional(12.0), Color32::WHITE);
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
        Kind::Upcoming => {
            let b = Rect::from_min_size(img.min + vec2(8.0, 8.0), vec2(88.0, 22.0));
            p.rect_filled(b, CornerRadius::same(6), accent());
            p.text(b.center(), Align2::CENTER_CENTER, "UPCOMING", FontId::proportional(12.0), ON_ACCENT);
        }
        Kind::Video if !v.duration.is_empty() => {
            let g = ui.fonts_mut(|f| f.layout_no_wrap(v.duration.clone(), FontId::proportional(12.0), Color32::WHITE));
            let b = Rect::from_min_size(img.right_bottom() - vec2(g.size().x + 18.0, 28.0), vec2(g.size().x + 12.0, 22.0));
            p.rect_filled(b, CornerRadius::same(6), Color32::from_black_alpha(190));
            p.text(b.center(), Align2::CENTER_CENTER, &v.duration, FontId::proportional(12.0), Color32::WHITE);
        }
        _ => {}
    }
    // hover play glyph
    if h > 0.0 {
        let c = img.center();
        p.circle_filled(c, 24.0 * (0.7 + 0.3 * h), Color32::from_black_alpha((150.0 * h) as u8));
        p.add(egui::Shape::convex_polygon(
            vec![c + vec2(-6.0, -10.0), c + vec2(-6.0, 10.0), c + vec2(10.0, 0.0)],
            Color32::WHITE.gamma_multiply(h),
            Stroke::NONE,
        ));
    }
    if info.is_new {
        let b = Rect::from_min_size(Pos2::new(img.max.x - 52.0, img.min.y + 8.0), vec2(44.0, 20.0));
        p.rect_filled(b, CornerRadius::same(6), accent());
        p.text(b.center(), Align2::CENTER_CENTER, "NEW", FontId::proportional(11.0), ON_ACCENT);
    }
    if let Some(wf) = info.watched {
        let pp = ui.painter();
        let track = Rect::from_min_size(Pos2::new(img.min.x, img.max.y - 4.0), vec2(img.width(), 4.0));
        pp.rect_filled(track, CornerRadius::ZERO, Color32::from_black_alpha(150));
        pp.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * wf.clamp(0.0, 1.0), 4.0)), CornerRadius::ZERO, accent());
    }
    if let Some(pr) = info.progress {
        let pp = ui.painter();
        let b = Rect::from_min_size(img.left_bottom() - vec2(-8.0, 30.0), vec2(96.0, 22.0));
        pp.rect_filled(b, CornerRadius::same(6), Color32::from_black_alpha(200));
        let fill = Rect::from_min_size(b.min, vec2(b.width() * pr.max(0.03), b.height()));
        pp.rect_filled(fill, CornerRadius::same(6), accent().gamma_multiply(0.85));
        icons::draw(pp, b.center() + vec2(-22.0, 0.0), "download", 12.0, Color32::WHITE);
        pp.text(b.center() + vec2(-10.0, 0.0), Align2::LEFT_CENTER, format!("{}%", (pr * 100.0) as u32), FontId::proportional(12.0), Color32::WHITE);
        let bar = Rect::from_min_size(Pos2::new(img.min.x, img.max.y - 3.0), vec2(img.width() * pr, 3.0));
        hgradient(pp, bar, accent(), accent2());
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
    // text
    let (tx, ty, tw) = if info.row {
        (img.max.x + 16.0, r.min.y + 12.0, rect.width() - img.width() - 40.0)
    } else {
        (r.min.x + 12.0, r.min.y + th + 6.0, rect.width() - 24.0)
    };
    let title = truncate(ui, &v.title, FontId::proportional(if info.row { 16.5 } else { 15.0 }), tw, 2);
    let p = ui.painter();
    p.text(Pos2::new(tx, ty), Align2::LEFT_TOP, title, FontId::proportional(if info.row { 16.5 } else { 15.0 }), ink());
    let sub_full = [v.channel_name.as_str(), v.meta.as_str()]
        .iter()
        .filter(|x| !x.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("  •  ");
    let sub = truncate(ui, &sub_full, FontId::proportional(12.5), tw, 1);
    ui.painter().text(Pos2::new(tx, r.max.y - 12.0), Align2::LEFT_BOTTOM, sub, FontId::proportional(12.5), muted());

    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut act: Option<CardAction> = None;
    resp.context_menu(|ui| {
        if ui.button(tr("Play audio only")).clicked() {
            act = Some(CardAction::PlayAudio);
            ui.close();
        }
        if v.kind != Kind::Live {
            if ui.button(tr("Play next")).clicked() {
                act = Some(CardAction::PlayNext);
                ui.close();
            }
            if ui.button(tr("Add to queue")).clicked() {
                act = Some(CardAction::AddToQueue);
                ui.close();
            }
        }
        if v.kind != Kind::Live {
            if info.video_saved {
                ui.add_enabled(false, egui::Button::new("Video downloaded"));
            } else {
                ui.menu_button("Download video", |ui| {
                    for (label, q) in [("Best available", 2160u32), ("1080p", 1080), ("720p", 720), ("480p", 480), ("360p", 360)] {
                        if ui.button(label).clicked() {
                            act = Some(CardAction::Download(false, q));
                            ui.close();
                        }
                    }
                });
            }
            if info.audio_saved {
                ui.add_enabled(false, egui::Button::new("Audio downloaded"));
            } else if ui.button(tr("Download audio only")).clicked() {
                act = Some(CardAction::Download(true, 0));
                ui.close();
            }
            ui.separator();
        }
        {
            let in_wl = info.lists.iter().any(|l| l.id == "watchlater" && l.videos.iter().any(|x| x.id == v.id));
            if ui.button(if in_wl { "Remove from Watch later" } else { "Watch later" }).clicked() {
                act = Some(CardAction::ToggleList("watchlater".into()));
                ui.close();
            }
        }
        ui.menu_button("Add to playlist", |ui| {
            for l in info.lists {
                let has = l.videos.iter().any(|x| x.id == v.id);
                if ui.selectable_label(has, l.name.clone()).clicked() {
                    act = Some(CardAction::ToggleList(l.id.clone()));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr("+  New playlist…")).clicked() {
                act = Some(CardAction::NewList);
                ui.close();
            }
        });
        if matches!(v.kind, Kind::Video) && !info.in_history_view {
            ui.separator();
            if ui.button(tr("Not interested")).clicked() {
                act = Some(CardAction::NotInterested);
                ui.close();
            }
            if !v.channel_id.is_empty() && ui.button(tr("Recommend less from this channel")).clicked() {
                act = Some(CardAction::FewerFromChannel);
                ui.close();
            }
        }
        if info.in_history_view && ui.button(tr("Remove from history")).clicked() {
            act = Some(CardAction::RemoveFromHistory);
            ui.close();
        }
        if info.current_list.is_some() && ui.button(tr("Remove from this playlist")).clicked() {
            act = Some(CardAction::RemoveFromList);
            ui.close();
        }
    });
    (resp.clicked(), h > 0.95, act)
}
