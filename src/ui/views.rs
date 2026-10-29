//! Shared pieces for the pages that show their items as a grid, a compact grid or a list
//! (Downloads, Clips, Podcasts), so they match the video pages.

use crate::*;

/// Sizes of one grid layout.
pub struct Metrics {
    pub cols: usize,
    pub cw: f32,
    pub img_h: f32,
    pub ch: f32,
    pub gap: f32,
    pub compact: bool,
}

/// Grid sizes for the chosen view. `square` is for album-style art (podcasts), otherwise 16:9.
pub fn metrics(settings: &Settings, avail: f32, square: bool) -> Metrics {
    let dens = [0.82f32, 1.0, 1.22][settings.density.min(2) as usize];
    let compact = settings.view_mode == 1;
    let gap = 16.0 * dens;
    let min_card = match (square, compact) {
        (false, false) => 250.0,
        (false, true) => 175.0,
        (true, false) => 170.0,
        (true, true) => 125.0,
    } * dens;
    let cols = (((avail + gap) / (min_card + gap)).floor() as usize).max(1);
    let cw = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
    let img_w = cw - 14.0;
    let img_h = if square { img_w } else { img_w * 9.0 / 16.0 };
    let text_h = if compact { 50.0 } else { 66.0 };
    Metrics { cols, cw, img_h, ch: img_h + 7.0 + text_h, gap, compact }
}

/// The three layout icons. Returns true when the choice changed.
pub fn view_switch(ui: &mut egui::Ui, mode: &mut u8) -> bool {
    let before = *mode;
    for (icon, m, tip) in [("view_list", 2u8, tr("List")), ("view_compact", 1, tr("Compact grid")), ("view_grid", 0, tr("Grid"))] {
        let (r, resp) = ui.allocate_exact_size(vec2(34.0, 30.0), Sense::click());
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
        let on = *mode == m;
        if on || resp.hovered() {
            ui.painter().rect_filled(r, CornerRadius::same(9), if on { accent().gamma_multiply(0.22) } else { ov(18) });
        }
        icons::draw(ui.painter(), r.center(), icon, 17.0, if on { accent() } else { muted() });
        if resp.clicked() {
            *mode = m;
        }
    }
    *mode != before
}

impl App {
    /// The layout switch as a stand-alone control (it saves the choice).
    pub fn view_toggle(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.settings.view_mode;
        if view_switch(ui, &mut mode) {
            self.settings.view_mode = mode;
            self.save_settings();
        }
    }
}

/// A card: panel, picture on top, title and one line of detail. Returns the picture's
/// rectangle so the caller can add badges and buttons over it.
pub fn media_card(ui: &mut egui::Ui, rect: Rect, m: &Metrics, uri: &str, title: &str, sub: &str, hv: f32) -> Rect {
    ui.painter().add(surface::panel(rect, 18.0, 0.8 + 0.2 * hv, if hv > 0.05 { Some(accent().gamma_multiply(0.3 * hv)) } else { None }, true));
    let img = Rect::from_min_size(rect.min + vec2(7.0, 7.0), vec2(rect.width() - 14.0, m.img_h));
    ui.painter().rect_filled(img, CornerRadius::same(12), card_hi());
    if !uri.is_empty() {
        egui::Image::new(uri.to_string()).fit_to_exact_size(img.size()).corner_radius(CornerRadius::same(12)).paint_at(ui, img);
    }
    let size = if m.compact { 13.5 } else { 14.5 };
    let lines = if m.compact { 1 } else { 2 };
    let t = truncate(ui, title, FontId::proportional(size), rect.width() - 22.0, lines);
    ui.painter().text(Pos2::new(rect.min.x + 11.0, img.max.y + 7.0), Align2::LEFT_TOP, t, FontId::proportional(size), ink());
    let s = truncate(ui, sub, FontId::proportional(12.0), rect.width() - 22.0, 1);
    ui.painter().text(Pos2::new(rect.min.x + 11.0, rect.max.y - 9.0), Align2::LEFT_BOTTOM, s, FontId::proportional(12.0), muted());
    img
}

/// A small dark pill with text over a picture (duration, kind …), anchored at its bottom-right.
pub fn pill_label(ui: &egui::Ui, bottom_right: Pos2, text: &str) {
    let g = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(11.5), Color32::WHITE));
    let b = Rect::from_min_size(bottom_right - vec2(g.size().x + 14.0, 22.0), vec2(g.size().x + 10.0, 18.0));
    ui.painter().rect_filled(b, CornerRadius::same(5), Color32::from_black_alpha(190));
    ui.painter().text(b.center(), Align2::CENTER_CENTER, text, FontId::proportional(11.5), Color32::WHITE);
}
