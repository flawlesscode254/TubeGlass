//! Looking for new releases on GitHub and installing them: when to check, what to do with the
//! answer, downloading the right file and (for an AppImage) replacing the running program.

use crate::*;
use crate::update::{self, Install};

/// How long a check stays valid before the next automatic one.
const RECHECK_SECS: u64 = 6 * 3600;

impl App {
    /// Called every frame: starts the automatic check when it is due (after start-up, then
    /// every few hours while the app stays open).
    pub(crate) fn update_tick(&mut self) {
        if !self.settings.update_check || !self.online || Instant::now() < self.update_next_check {
            return;
        }
        self.update_next_check = Instant::now() + Duration::from_secs(RECHECK_SECS);
        // a check made recently (by an earlier run) is still fresh
        if now_secs().saturating_sub(self.settings.last_update_check) < RECHECK_SECS {
            return;
        }
        self.check_for_update(false);
    }

    /// Asks GitHub for the newest release in the background. `manual` is true when the user
    /// pressed "Check now" (so "you are up to date" is worth saying).
    pub(crate) fn check_for_update(&mut self, manual: bool) {
        if self.update_checking {
            return;
        }
        self.update_checking = true;
        self.update_note = None;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let r = update::fetch_latest();
            let _ = tx.send(Msg::UpdateChecked(r, manual));
            ctx.request_repaint();
        });
    }

    /// Handles the answer of a check.
    pub(crate) fn apply_update_check(&mut self, r: Result<Option<update::Release>, String>, manual: bool) {
        self.update_checking = false;
        match r {
            Ok(found) => {
                self.settings.last_update_check = now_secs();
                self.save_settings();
                let newer = found.filter(|r| update::is_newer(&r.version, update::CURRENT));
                match newer {
                    Some(rel) => {
                        // a skipped version stays quiet unless the user asked
                        if manual || rel.version != self.settings.skipped_version {
                            self.update_note = Some(trf("Version {version} is available", &[("version", &rel.version)]));
                            if self.update.as_ref().map(|u| &u.version) != Some(&rel.version) {
                                self.update_state = UpdateState::Idle;
                            }
                            self.update = Some(rel);
                        } else {
                            self.update_note = Some(trf("Version {version} is available (you skipped it)", &[("version", &rel.version)]));
                        }
                    }
                    None => {
                        self.update = None;
                        self.update_note = Some(trf("You have the latest version ({version})", &[("version", update::CURRENT)]));
                    }
                }
            }
            Err(e) => {
                if manual {
                    self.update_note = Some(trf("Could not check for updates: {error}", &[("error", &e)]));
                }
            }
        }
    }

    /// "Update": downloads the file that fits this installation, or opens the release page when
    /// there is none.
    pub(crate) fn start_update(&mut self) {
        let Some(rel) = self.update.clone() else { return };
        let install = update::detect_install();
        let Some(asset) = update::pick_asset(&rel.assets, install, std::env::consts::ARCH).cloned() else {
            // nothing built for this computer: let the user pick on the release page
            if !rel.page.is_empty() {
                crate::platform::open_url(&rel.page);
            }
            return;
        };
        if matches!(self.update_state, UpdateState::Downloading(_)) {
            return;
        }
        self.update_state = UpdateState::Downloading(0.0);
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(-1i32);
            let r = update::download(&asset, &|p| {
                let pct = (p * 100.0) as i32;
                if pct != last.get() {
                    last.set(pct);
                    let _ = tx.send(Msg::UpdateProgress(p));
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Msg::UpdateDownloaded(r));
            ctx.request_repaint();
        });
    }

    /// The file is downloaded: install it where that is possible, otherwise show it.
    pub(crate) fn finish_update(&mut self, r: Result<std::path::PathBuf, String>) {
        match r {
            Err(e) => self.update_state = UpdateState::Failed(e),
            Ok(path) => match update::detect_install() {
                Install::AppImage => match update::replace_appimage(&path) {
                    Ok(_) => {
                        let _ = std::fs::remove_file(&path);
                        self.update_state = UpdateState::Done(path, true);
                    }
                    Err(e) => self.update_state = UpdateState::Failed(e),
                },
                // a tarball install updates itself when its folder is writable
                Install::LinuxOther if update::install_tarball(&path).is_ok() => {
                    let _ = std::fs::remove_file(&path);
                    self.update_state = UpdateState::Done(path, true);
                }
                // a .deb opens in the software installer; archives open in the file manager
                _ => {
                    crate::platform::open_path(&path);
                    self.update_state = UpdateState::Done(path, false);
                }
            },
        }
    }

    /// "Skip this version": no more reminders until a newer one appears.
    pub(crate) fn skip_update(&mut self) {
        if let Some(u) = self.update.take() {
            self.settings.skipped_version = u.version;
            self.save_settings();
        }
        self.update_state = UpdateState::Idle;
    }
}
