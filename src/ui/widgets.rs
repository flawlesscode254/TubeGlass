//! Small reusable widgets: text fields, icon buttons, switches, progress pills.

use crate::*;

/// A -10…+10 dB slider with a visible track. True when the value was changed or released.
pub(crate) fn db_slider(ui: &mut egui::Ui, v: &mut f32, width: f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let (l, rr) = (r.min.x + 10.0, r.max.x - 10.0);
    let mut done = false;
    if let (true, Some(p)) = (resp.is_pointer_button_down_on() || resp.clicked(), resp.interact_pointer_pos()) {
        *v = (((p.x - l) / (rr - l)).clamp(0.0, 1.0) * 20.0 - 10.0).round();
    }
    if resp.drag_stopped() || resp.clicked() {
        done = true;
    }
    let cy = r.center().y;
    let mid = (l + rr) / 2.0;
    let x = l + (*v + 10.0) / 20.0 * (rr - l);
    let p = ui.painter();
    p.rect_filled(Rect::from_min_max(Pos2::new(l, cy - 2.0), Pos2::new(rr, cy + 2.0)), CornerRadius::same(2), ov(34));
    p.rect_filled(Rect::from_min_max(Pos2::new(mid.min(x), cy - 2.0), Pos2::new(mid.max(x), cy + 2.0)), CornerRadius::same(2), accent());
    p.line_segment([Pos2::new(mid, cy - 6.0), Pos2::new(mid, cy + 6.0)], Stroke::new(1.0, ov(70)));
    p.circle_filled(Pos2::new(x, cy), if resp.hovered() { 9.0 } else { 8.0 }, Color32::WHITE);
    ui.label(egui::RichText::new(format!("{:+.0} dB", *v)).size(13.0).color(muted()));
    done
}

/// What a text field reports back.
pub(crate) struct FieldOut {
    pub(crate) resp: egui::Response,
    pub(crate) rect: Rect,
    pub(crate) submit: bool,
    pub(crate) action: bool,
    pub(crate) cleared: bool,
}

/// The one text-input design used across the app: a capsule with a leading icon, the
/// text vertically centred, a clear button, an accent focus ring and an optional action
/// button on the right.
pub(crate) fn text_field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    width: f32,
    height: f32,
    icon: &str,
    action: Option<&str>,
    busy: bool,
) -> FieldOut {
    let (r, _) = ui.allocate_exact_size(vec2(width.max(height * 2.0), height), Sense::hover());
    let bg = ui.painter().add(egui::Shape::Noop);
    let font = FontId::proportional(if height >= 44.0 { 16.0 } else { 15.0 });
    let act_w = match action {
        Some(l) => {
            let tw = ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), FontId::proportional(14.0), ink()).size().x);
            tw + 30.0
        }
        None => 0.0,
    };
    let has_text = !text.is_empty();
    let clear_w = if has_text { 26.0 } else { 0.0 };
    let pad = 6.0;
    let left = r.min.x + height * 0.5 + 16.0;
    let act_gap = if act_w > 0.0 { 6.0 } else { 0.0 };
    let right = (r.max.x - pad - act_w - act_gap - clear_w - 6.0).max(left + 24.0);
    let edit_rect = Rect::from_min_max(Pos2::new(left, r.min.y + 2.0), Pos2::new(right, r.max.y - 2.0));
    let te = egui::TextEdit::singleline(text)
        .hint_text(egui::RichText::new(hint).color(muted()))
        .font(font)
        .align(Align2::LEFT_CENTER)
        .margin(egui::Margin::ZERO)
        .frame(egui::Frame::NONE)
        .min_size(edit_rect.size())
        .desired_width(edit_rect.width());
    let resp = ui.put(edit_rect, te);
    let focused = resp.has_focus();
    ui.painter().set(bg, surface::capsule(r, 1.0, if focused { Some(accent()) } else { None }, false));
    icons::draw(
        ui.painter(),
        Pos2::new(r.min.x + height * 0.5 + 2.0, r.center().y),
        icon,
        16.0,
        if focused { accent() } else { icons::muted_icon() },
    );
    let mut out = FieldOut { rect: r, submit: false, action: false, cleared: false, resp };
    if out.resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        out.submit = true;
    }
    if has_text {
        let xr = Rect::from_center_size(
            Pos2::new(r.max.x - pad - act_w - act_gap - clear_w / 2.0 - 2.0, r.center().y),
            vec2(24.0, 24.0),
        );
        let xresp = ui
            .interact(xr, out.resp.id.with("clear"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if xresp.hovered() {
            ui.painter().circle_filled(xr.center(), 12.0, ov(26));
        }
        icons::draw(ui.painter(), xr.center(), "close", 12.0, if xresp.hovered() { ink() } else { icons::muted_icon() });
        if xresp.clicked() {
            text.clear();
            out.cleared = true;
        }
    }
    if let Some(label) = action {
        let ar = Rect::from_min_max(
            Pos2::new(r.max.x - pad - act_w, r.min.y + 6.0),
            Pos2::new(r.max.x - pad, r.max.y - 6.0),
        );
        if busy {
            ui.put(Rect::from_center_size(ar.center(), vec2(18.0, 18.0)), egui::Spinner::new().color(accent()));
        } else {
            let aresp = ui
                .interact(ar, out.resp.id.with("action"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let fill = if aresp.hovered() { accent2() } else { accent() };
            ui.painter().rect_filled(ar, CornerRadius::same((ar.height() / 2.0) as u8), fill);
            ui.painter().text(ar.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), ON_ACCENT);
            if aresp.clicked() {
                out.action = true;
            }
        }
    }
    out
}

/// Draws `icon` followed by `label` (and an optional chevron), centred on `center`.
/// One comment (or reply). Returns true when its "replies" button was clicked.
pub(crate) fn comment_row(ui: &mut egui::Ui, c: &Comment, small: bool, replies_open: bool) -> bool {
    let av = if small { 26.0 } else { 36.0 };
    let mut clicked = false;
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(av, av), Sense::hover());
        ui.painter().circle_filled(r.center(), av / 2.0, card_hi());
        let uri = img_uri(ui.ctx(), &c.avatar);
        if !uri.is_empty() {
            egui::Image::new(uri)
                .corner_radius(CornerRadius::same((av / 2.0) as u8))
                .paint_at(ui, r);
        }
        ui.vertical(|ui| {
            if let Some(p) = &c.pinned {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 16.0), Sense::hover());
                    icons::draw(ui.painter(), r.center(), "pin", 13.0, accent());
                    ui.label(egui::RichText::new(p).size(11.5).color(accent()));
                });
            }
            ui.horizontal(|ui| {
                if c.by_creator {
                    let g = ui.fonts_mut(|f| f.layout_no_wrap(c.author.clone(), FontId::proportional(13.5), ON_ACCENT));
                    let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 14.0, 20.0), Sense::hover());
                    ui.painter().rect_filled(r, CornerRadius::same(10), accent());
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, &c.author, FontId::proportional(if small { 12.5 } else { 13.5 }), ON_ACCENT);
                } else {
                    ui.label(egui::RichText::new(&c.author).strong().size(if small { 12.5 } else { 13.5 }));
                }
                if c.verified {
                    let (r, _) = ui.allocate_exact_size(vec2(13.0, 16.0), Sense::hover());
                    icons::draw(ui.painter(), r.center(), "check", 11.0, muted());
                }
                ui.label(egui::RichText::new(&c.time).size(12.0).color(muted()));
            });
            ui.add(egui::Label::new(egui::RichText::new(&c.text).size(if small { 13.5 } else { 14.0 })).wrap());
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // likes
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
                icons::draw(ui.painter(), r.center(), "thumb_up", 15.0, muted());
                let likes = if c.likes.is_empty() { "0" } else { c.likes.as_str() };
                ui.label(egui::RichText::new(likes).size(12.5).color(muted()));
                if let Some(h) = &c.hearted {
                    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    icons::draw(ui.painter(), r.center(), "heart_fill", 14.0, LIVE);
                    resp.on_hover_text(h);
                }
                // replies toggle
                if c.reply_token.is_some() {
                    ui.add_space(10.0);
                    let label = if c.replies.is_empty() || c.replies == "0" {
                        tr("Replies")
                    } else if c.replies == "1" {
                        trf("{count} reply", &[("count", &c.replies)])
                    } else {
                        trf("{count} replies", &[("count", &c.replies)])
                    };
                    let tw = ui.fonts_mut(|f| {
                        f.layout_no_wrap(label.clone(), FontId::proportional(13.0), ink()).size().x
                    });
                    let (r, resp) = ui.allocate_exact_size(vec2(tw + 52.0, 24.0), Sense::click());
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let hv = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
                    let bg = if replies_open { accent().gamma_multiply(0.30) } else { ov((10.0 + 18.0 * hv) as u8) };
                    ui.painter().rect_filled(r, CornerRadius::same(12), bg);
                    icons::draw(ui.painter(), Pos2::new(r.min.x + 15.0, r.center().y), "reply", 14.0, if replies_open { Color32::WHITE } else { icons::muted_icon() });
                    ui.painter().text(Pos2::new(r.min.x + 28.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), if replies_open { Color32::WHITE } else { ink().gamma_multiply(0.9) });
                    icons::draw(
                        ui.painter(),
                        Pos2::new(r.max.x - 12.0, r.center().y),
                        if replies_open { "chevron_up" } else { "chevron_down" },
                        11.0,
                        if replies_open { Color32::WHITE } else { icons::muted_icon() },
                    );
                    if resp.clicked() {
                        clicked = true;
                    }
                }
            });
        });
    });
    clicked
}

/// A button with a vector icon (and optional dropdown chevron) in front of its label, so it
/// looks the same everywhere instead of depending on which font has the glyph.
pub(crate) fn icon_btn(ui: &mut egui::Ui, icon: &str, label: &str, fill: Option<Color32>, fg: Color32, chevron: bool, enabled: bool) -> egui::Response {
    let font = FontId::proportional(14.0);
    let tw = if label.is_empty() { 0.0 } else { ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font, fg).size().x) };
    let icon_w = if icon.is_empty() { 0.0 } else { 16.0 };
    let gap = if icon.is_empty() || label.is_empty() { 0.0 } else { 7.0 };
    let chev = if chevron { 14.0 } else { 0.0 };
    let size = vec2(icon_w + gap + tw + chev + 26.0, 30.0);
    let (r, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let resp = if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    let hv = enabled && resp.hovered();
    let bg = match fill {
        Some(f) if hv => f.gamma_multiply(1.12),
        Some(f) => f,
        None if hv => card_hi(),
        None => card(),
    };
    let fade = if enabled { 1.0 } else { 0.45 };
    ui.painter().rect_filled(r, CornerRadius::same(16), bg.gamma_multiply(fade));
    if fill.is_none() {
        ui.painter().rect_stroke(r, CornerRadius::same(16), Stroke::new(1.0, if hv { accent() } else { ov(26) }), StrokeKind::Inside);
    }
    icon_label(ui, r.center(), icon, label, fg.gamma_multiply(fade), chevron);
    resp
}

/// A progress bar with its label inside; the text is dark over the filled part and light over
/// the empty part, so it stays readable wherever the fill happens to be.
pub(crate) fn progress_pill(ui: &mut egui::Ui, frac: f32, label: &str, width: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let frac = frac.clamp(0.0, 1.0);
    ui.painter().rect_filled(r, CornerRadius::same(15), card_hi());
    let fill = Rect::from_min_size(r.min, vec2((r.width() * frac).max(if frac > 0.0 { 12.0 } else { 0.0 }), r.height()));
    if fill.width() > 0.0 {
        let clip = ui.painter().with_clip_rect(fill);
        clip.rect_filled(r, CornerRadius::same(15), accent());
    }
    let font = FontId::proportional(13.5);
    let c = r.center();
    let over_fill = ui.painter().with_clip_rect(fill.intersect(r));
    over_fill.text(c, Align2::CENTER_CENTER, label, font.clone(), ON_ACCENT);
    let rest = Rect::from_min_max(Pos2::new(fill.max.x, r.min.y), r.max);
    ui.painter().with_clip_rect(rest).text(c, Align2::CENTER_CENTER, label, font, ink());
}

/// Draws an icon followed by text (and an optional chevron), centred on a point.
pub(crate) fn icon_label(ui: &egui::Ui, center: Pos2, icon: &str, label: &str, col: Color32, chevron: bool) {
    let font = FontId::proportional(14.0);
    let tw = if label.is_empty() {
        0.0
    } else {
        ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), col).size().x)
    };
    let icon_w = if icon.is_empty() { 0.0 } else { 16.0 };
    let gap_i = if icon.is_empty() || label.is_empty() { 0.0 } else { 7.0 };
    let chev_w = if chevron { 14.0 } else { 0.0 };
    let total = icon_w + gap_i + tw + chev_w;
    let mut x = center.x - total / 2.0;
    if !icon.is_empty() {
        icons::draw(ui.painter(), Pos2::new(x + icon_w / 2.0, center.y), icon, 16.0, col);
        x += icon_w + gap_i;
    }
    if !label.is_empty() {
        ui.painter().text(Pos2::new(x, center.y), Align2::LEFT_CENTER, label, font, col);
        x += tw;
    }
    if chevron {
        icons::draw(ui.painter(), Pos2::new(x + 8.0, center.y + 1.0), "chevron_down", 12.0, col);
    }
}

/// iOS-style switch. Returns true when it was clicked.
pub(crate) fn switch(ui: &mut egui::Ui, on: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let k = ui.ctx().animate_bool_with_time(resp.id, *on, 0.14);
    let track = if *on { accent() } else { ov(38) };
    ui.painter().rect_filled(r, CornerRadius::same(12), track);
    let x = r.min.x + 12.0 + (r.width() - 24.0) * k;
    ui.painter().circle_filled(Pos2::new(x, r.center().y), 9.0, if *on { ON_ACCENT } else { Color32::WHITE });
    if resp.clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

/// Wraps text to a width and line count, ending with an ellipsis when it does not fit.
pub(crate) fn truncate(ui: &egui::Ui, text: &str, font: FontId, max_w: f32, max_lines: usize) -> String {
    // Greedy word wrap with ellipsis, measured with the real font.
    let mut lines: Vec<String> = vec![];
    let mut cur = String::new();
    let measure = |s: &str| ui.fonts_mut(|f| f.layout_no_wrap(s.to_string(), font.clone(), ink()).size().x);
    for word in text.split_whitespace() {
        let trial = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if measure(&trial) <= max_w {
            cur = trial;
        } else {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            cur = word.to_string();
            if lines.len() == max_lines {
                break;
            }
        }
    }
    if lines.len() < max_lines && !cur.is_empty() {
        lines.push(cur);
        cur = String::new();
    }
    let overflow = !cur.is_empty() || lines.len() > max_lines;
    lines.truncate(max_lines);
    if overflow {
        if let Some(last) = lines.last_mut() {
            while measure(&format!("{last}…")) > max_w && !last.is_empty() {
                last.pop();
            }
            last.push('…');
        }
    }
    lines.join("\n")
}
