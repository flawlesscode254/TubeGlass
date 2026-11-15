//! Making sure ffmpeg is there: checked at launch and, when it is missing, downloaded once.

use crate::*;

impl App {
    /// Downloads a pinned ffmpeg build in the background (progress shows in the bar at the top).
    pub(crate) fn start_ffmpeg_setup(&mut self) {
        if matches!(self.ffmpeg, FfmpegState::Downloading(_)) {
            return;
        }
        self.ffmpeg = FfmpegState::Downloading(0.0);
        self.ffmpeg_hidden = false;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(-1i32);
            let r = ffmpeg_setup::install(&|p| {
                let pct = (p * 100.0) as i32;
                if pct != last.get() {
                    last.set(pct);
                    let _ = tx.send(Msg::FfmpegProgress(p));
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::FfmpegInstalled(r));
            ctx.request_repaint();
        });
    }

    /// The launch check found no ffmpeg: fetch one (unless this system has no ready-made build).
    pub(crate) fn ffmpeg_checked(&mut self, found: bool) {
        if found {
            self.ffmpeg = FfmpegState::Ready;
        } else if ffmpeg_setup::source_for_this_system().is_some() {
            // a copy that came with the package but will not start must not be chosen again
            crate::platform::reject_ffmpeg();
            self.start_ffmpeg_setup();
        } else {
            self.ffmpeg = FfmpegState::Failed(trf("No ready-made ffmpeg exists for {platform}. {hint}", &[("platform", &ffmpeg_setup::platform_key()), ("hint", &crate::platform::ffmpeg_hint())]));
        }
    }

    /// The download finished or failed.
    pub(crate) fn ffmpeg_installed(&mut self, r: Result<std::path::PathBuf, String>) {
        match r {
            Ok(_) => {
                self.ffmpeg = FfmpegState::Ready;
                self.toast(tr("ffmpeg is ready"));
                // a video that failed only because ffmpeg was missing can be played now
                if matches!(self.player.state(), State::Error(_)) {
                    self.want_retry = true;
                }
            }
            Err(e) => self.ffmpeg = FfmpegState::Failed(e),
        }
    }
}
