//! Keyboard shortcuts while something is playing.

use crate::*;

impl App {
    /// Handles the keyboard shortcuts of the player.
    pub(crate) fn keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.watching.is_none() {
            return;
        }
        use egui::Key::*;
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        let live = self.player.is_live();
        if pressed(Space) || pressed(K) {
            self.player.toggle_pause();
        }
        if !live {
            let p = self.player.position();
            if pressed(ArrowLeft) { self.player.seek(p - 5.0); }
            if pressed(ArrowRight) { self.player.seek(p + 5.0); }
            if pressed(J) { self.player.seek(p - 10.0); }
            if pressed(L) { self.player.seek(p + 10.0); }
        }
        if pressed(ArrowUp) { self.player.set_volume(self.player.volume + 0.05); }
        if pressed(ArrowDown) { self.player.set_volume(self.player.volume - 0.05); }
        if pressed(M) { self.player.toggle_mute(); }
        if pressed(F) { self.toggle_fullscreen(); }
        if pressed(Questionmark) {
            self.show_help = !self.show_help;
        }
        if pressed(N) {
            self.queue_next();
        }
        if pressed(P) {
            self.queue_prev();
        }
        if pressed(Q) {
            if self.comments_open && self.side_tab == SideTab::Queue {
                self.comments_open = false;
            } else {
                self.side_tab = SideTab::Queue;
                self.comments_open = true;
            }
        }
        if pressed(B) {
            self.settings.ambient = !self.settings.ambient;
            self.save_settings();
            self.toast(tr(if self.settings.ambient { "Ambient mode on" } else { "Ambient mode off" }));
        }
        if pressed(W) {
            if let Some(w) = self.watching.clone().filter(|w| !w.id.starts_with("local:")) {
                self.toggle_in_list("watchlater", &w);
                let on = self.lists.iter().any(|l| l.id == "watchlater" && l.videos.iter().any(|x| x.id == w.id));
                self.toast(tr(if on { "Added to Watch later" } else { "Removed from Watch later" }));
            }
        }
        if pressed(Y) && self.watching.as_ref().is_some_and(|w| w.music) {
            if self.comments_open && self.side_tab == SideTab::Lyrics {
                self.comments_open = false;
            } else {
                self.side_tab = SideTab::Lyrics;
                self.comments_open = true;
            }
        }
        if pressed(C) { self.toggle_comments(); }
        if pressed(OpenBracket) {
            let v = (self.player.speed - 0.25).max(0.5);
            self.player.set_speed(v);
            self.settings.speed = v;
            self.save_settings();
            self.toast(trf("Speed {speed}×", &[("speed", &v.to_string())]));
        }
        if pressed(CloseBracket) {
            let v = (self.player.speed + 0.25).min(2.0);
            self.player.set_speed(v);
            self.settings.speed = v;
            self.save_settings();
            self.toast(trf("Speed {speed}×", &[("speed", &v.to_string())]));
        }
        if pressed(I) {
            let on = !self.mini;
            self.set_mini(on);
        }
        if pressed(A) {
            let on = !self.player.audio_mode;
            self.player.set_audio_mode(on);
        }
        if pressed(Escape) && !self.pip {
            if self.fullscreen { self.toggle_fullscreen(); } else { self.back_from_player(); }
        }
        // a key press shows the controls for a moment, like moving the mouse does
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
            self.last_move = Instant::now();
        }
    }
}
