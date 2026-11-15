//! Cached blurred cover backdrop and geometry for the audio-only player.

use std::sync::{Arc, mpsc};

use eframe::egui::{self, Color32, ColorImage, Context, Painter, Rect, TextureHandle, pos2, vec2};

const MAX_BLUR_EDGE: u32 = 192;
const BLUR_SIGMA: f32 = 12.0;

#[derive(Clone)]
struct ImageKey {
    identity: String,
    image: Arc<ColorImage>,
}

impl ImageKey {
    fn same(&self, other: &Self) -> bool {
        self.identity == other.identity && Arc::ptr_eq(&self.image, &other.image)
    }
}

struct Worker {
    key: ImageKey,
    rx: mpsc::Receiver<Option<ColorImage>>,
}

#[derive(Default)]
pub(crate) struct MusicBackdrop {
    desired: Option<ImageKey>,
    worker: Option<Worker>,
    texture: Option<TextureHandle>,
    enabled: bool,
    failed: Option<ImageKey>,
}

impl MusicBackdrop {
    pub(crate) fn update(
        &mut self,
        ctx: &Context,
        identity: &str,
        image: Option<Arc<ColorImage>>,
        enabled: bool,
    ) {
        let next = image.map(|image| ImageKey {
            identity: identity.to_owned(),
            image,
        });
        self.enabled = enabled;
        let changed = match (&self.desired, &next) {
            (Some(a), Some(b)) => !a.same(b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.desired = next;
            self.texture = None;
            self.failed = None;
        }

        if let Some(result) = self.worker.as_ref().map(|job| job.rx.try_recv()) {
            match result {
                Ok(result) => {
                    let worker = self.worker.take().expect("music_worker");
                    if self
                        .desired
                        .as_ref()
                        .is_some_and(|key| key.same(&worker.key))
                    {
                        if let Some(pixels) = result {
                            self.texture = Some(ctx.load_texture(
                                "music-stage-blur",
                                pixels,
                                egui::TextureOptions::LINEAR,
                            ));
                        } else {
                            self.remember_failure(worker.key);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let worker = self.worker.take().expect("music_worker");
                    if self
                        .desired
                        .as_ref()
                        .is_some_and(|key| key.same(&worker.key))
                    {
                        self.remember_failure(worker.key);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        if !enabled || self.worker.is_some() || self.texture.is_some() {
            return;
        }
        let Some(key) = self.desired.as_ref() else {
            return;
        };
        if self.failed.as_ref().is_some_and(|bad| bad.same(key)) {
            return;
        }
        let key = key.clone();
        let image = Arc::clone(&key.image);
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = blurred_image(&image);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.worker = Some(Worker { key, rx });
    }

    fn remember_failure(&mut self, key: ImageKey) {
        self.failed = Some(key);
    }

    pub(crate) fn paint(&self, painter: &Painter, rect: Rect) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        painter.rect_filled(rect, 0.0, Color32::BLACK);
        if !self.enabled {
            return;
        }
        let Some(texture) = self.texture.as_ref() else {
            return;
        };
        let [w, h] = texture.size();
        if w == 0 || h == 0 {
            return;
        }
        let view_aspect = rect.width() / rect.height();
        let image_aspect = w as f32 / h as f32;
        let uv = if image_aspect > view_aspect {
            let span = view_aspect / image_aspect;
            Rect::from_min_max(pos2((1.0 - span) * 0.5, 0.0), pos2((1.0 + span) * 0.5, 1.0))
        } else {
            let span = image_aspect / view_aspect;
            Rect::from_min_max(pos2(0.0, (1.0 - span) * 0.5), pos2(1.0, (1.0 + span) * 0.5))
        };
        painter.image(texture.id(), rect, uv, Color32::WHITE);
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(200));
        let edge_h = rect.height() * 0.25;
        crate::vgradient(
            painter,
            Rect::from_min_size(rect.min, vec2(rect.width(), edge_h)),
            Color32::from_black_alpha(65),
            Color32::TRANSPARENT,
        );
        crate::vgradient(
            painter,
            Rect::from_min_size(
                pos2(rect.left(), rect.bottom() - edge_h),
                vec2(rect.width(), edge_h),
            ),
            Color32::TRANSPARENT,
            Color32::from_black_alpha(90),
        );
    }
}

fn blurred_image(source: &ColorImage) -> Option<ColorImage> {
    let [width, height] = source.size;
    if width == 0 || height == 0 || source.pixels.len() != width.checked_mul(height)? {
        return None;
    }
    let w = u32::try_from(width).ok()?;
    let h = u32::try_from(height).ok()?;
    let longest = w.max(h);
    let nw = ((w as u64 * MAX_BLUR_EDGE as u64) / longest as u64)
        .max(1)
        .min(w as u64) as u32;
    let nh = ((h as u64 * MAX_BLUR_EDGE as u64) / longest as u64)
        .max(1)
        .min(h as u64) as u32;
    // Sample directly into the bounded buffer. In particular, do not duplicate a
    // full-resolution local cover merely to create its tiny blurred backdrop.
    let picture = image::RgbaImage::from_fn(nw, nh, |x, y| {
        let sx = ((x as u64 * w as u64 + w as u64 / 2) / nw as u64).min(w as u64 - 1) as usize;
        let sy = ((y as u64 * h as u64 + h as u64 / 2) / nh as u64).min(h as u64 - 1) as usize;
        image::Rgba(source.pixels[sy * width + sx].to_srgba_unmultiplied())
    });
    let blurred = image::imageops::blur(&picture, BLUR_SIGMA);
    Some(ColorImage::from_rgba_unmultiplied(
        [blurred.width() as usize, blurred.height() as usize],
        blurred.as_raw(),
    ))
}

/// Respect the galley's own alignment origin, including centered wrapped rows.
pub(crate) fn paint_lyric(painter: &Painter, rect: Rect, y: f32, galley: Arc<egui::Galley>) {
    let origin = pos2(rect.center().x - galley.rect.center().x, y);
    painter.galley(origin, galley, Color32::WHITE);
}

/// Positions the art, spectrum and lyric block inside the stage safe area.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StageLayout {
    pub(crate) art: Rect,
    pub(crate) spectrum: Rect,
    pub(crate) lyrics: Rect,
}

impl StageLayout {
    pub(crate) fn new(full: Rect, art_aspect: f32, lyrics_height: f32) -> Self {
        let aspect = if art_aspect.is_finite() && art_aspect > 0.0 {
            art_aspect
        } else {
            1.0
        };
        let width = (full.width() - 40.0).max(0.0);
        let top = (full.top() + 80.0).min(full.bottom());
        let bottom = (full.bottom() - 110.0).max(top);
        let budget = bottom - top;
        let lyrics_h = if lyrics_height.is_finite() {
            lyrics_height.max(0.0).min(budget)
        } else {
            0.0
        };
        let with_lyrics = lyrics_h > 0.0;
        let gaps = if with_lyrics { 2.0 } else { 1.0 };
        let gap = 22.0_f32.min(((budget - lyrics_h) / (gaps + 2.0)).max(0.0));

        let mut art_h = 300.0_f32.min(width.min(360.0) / aspect);
        let mut art_w = art_h * aspect;
        let mut spectrum_h = 90.0;
        let needed = art_h + spectrum_h;
        let space = (budget - lyrics_h - gaps * gap).max(0.0);
        if needed > space && needed > 0.0 {
            let scale = space / needed;
            art_h *= scale;
            art_w *= scale;
            spectrum_h *= scale;
        }
        let total = art_h + spectrum_h + lyrics_h + gaps * gap;
        let mut y = top + (budget - total).max(0.0) * 0.5;
        let cx = full.center().x;
        let art = Rect::from_min_size(pos2(cx - art_w * 0.5, y), vec2(art_w, art_h));
        y += art_h + gap;
        let spectrum_w = width.min(520.0);
        let spectrum =
            Rect::from_min_size(pos2(cx - spectrum_w * 0.5, y), vec2(spectrum_w, spectrum_h));
        y += spectrum_h + if with_lyrics { gap } else { 0.0 };
        let lyrics_w = width.min(760.0);
        let lyrics = Rect::from_min_size(pos2(cx - lyrics_w * 0.5, y), vec2(lyrics_w, lyrics_h));
        Self {
            art,
            spectrum,
            lyrics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover() -> Arc<ColorImage> {
        Arc::new(ColorImage::filled([32, 32], Color32::WHITE))
    }

    #[test]
    fn stale_worker_is_rejected_and_disabled_does_not_start_work() {
        let ctx = Context::default();
        let old = cover();
        let new = cover();
        let (tx, rx) = mpsc::channel();
        let key = ImageKey {
            identity: "old".into(),
            image: old,
        };
        let mut cache = MusicBackdrop {
            desired: Some(key.clone()),
            worker: Some(Worker { key, rx }),
            ..Default::default()
        };
        tx.send(Some(ColorImage::filled([4, 4], Color32::RED)))
            .unwrap();
        cache.update(&ctx, "new", Some(new), false);
        assert!(cache.texture.is_none(), "stale_result");
        assert!(cache.worker.is_none(), "disabled_submission");
    }

    #[test]
    fn ready_texture_is_reused_until_decoded_image_changes() {
        let ctx = Context::default();
        let image = cover();
        let key = ImageKey {
            identity: "cover".into(),
            image: image.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let mut cache = MusicBackdrop {
            desired: Some(key.clone()),
            worker: Some(Worker { key, rx }),
            ..Default::default()
        };
        tx.send(Some(ColorImage::filled([4, 4], Color32::RED)))
            .unwrap();
        cache.update(&ctx, "cover", Some(image.clone()), true);
        let id = cache.texture.as_ref().unwrap().id();
        for _ in 0..20 {
            cache.update(&ctx, "cover", Some(image.clone()), true);
            assert_eq!(cache.texture.as_ref().unwrap().id(), id);
            assert!(cache.worker.is_none());
        }
        cache.update(&ctx, "cover", Some(cover()), false);
        assert!(cache.texture.is_none(), "decoded_identity");
    }

    #[test]
    fn failed_work_is_not_repeated_and_pending_artwork_can_arrive_later() {
        let ctx = Context::default();
        let image = cover();
        let key = ImageKey {
            identity: "cover".into(),
            image: image.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let mut cache = MusicBackdrop {
            desired: Some(key.clone()),
            worker: Some(Worker { key, rx }),
            ..Default::default()
        };
        tx.send(None).unwrap();
        cache.update(&ctx, "cover", Some(image.clone()), true);
        cache.update(&ctx, "cover", Some(image.clone()), true);
        assert!(cache.worker.is_none());
        assert!(cache.failed.is_some());
        cache.update(&ctx, "loading", None, true);
        assert!(cache.failed.is_none());
        cache.update(&ctx, "loading", Some(image), true);
        assert!(cache.worker.is_some());
    }

    #[test]
    fn rapid_changes_coalesce_behind_one_worker() {
        let ctx = Context::default();
        let image = cover();
        let key = ImageKey {
            identity: "first".into(),
            image: image.clone(),
        };
        let (tx, rx) = mpsc::channel();
        let mut cache = MusicBackdrop {
            desired: Some(key.clone()),
            worker: Some(Worker { key, rx }),
            ..Default::default()
        };
        for identity in ["second", "third", "latest"] {
            cache.update(&ctx, identity, Some(image.clone()), true);
            assert_eq!(cache.worker.as_ref().unwrap().key.identity, "first");
        }
        tx.send(Some(ColorImage::filled([4, 4], Color32::RED)))
            .unwrap();
        cache.update(&ctx, "latest", Some(image), true);
        assert!(cache.texture.is_none());
        assert_eq!(cache.worker.as_ref().unwrap().key.identity, "latest");
    }

    #[test]
    fn blur_is_bounded_and_rejects_invalid_pixels() {
        let source = ColorImage::filled([640, 360], Color32::WHITE);
        let blurred = blurred_image(&source).unwrap();
        assert_eq!(blurred.size, [192, 108]);
        assert!(blurred_image(&ColorImage::filled([0, 0], Color32::WHITE)).is_none());
    }

    #[test]
    fn wrapped_lyric_rows_remain_centered_inside_the_stage() {
        let ctx = Context::default();
        let stage = Rect::from_min_size(pos2(300.0, 200.0), vec2(430.0, 150.0));
        let output = ctx.run_pass(Default::default(), |ctx| {
            let text = ["we", "drift", "beneath", "violet", "skies"]
                .repeat(5)
                .join(" ");
            let mut job = egui::text::LayoutJob::simple(
                text,
                egui::FontId::proportional(28.0),
                Color32::WHITE,
                stage.width(),
            );
            job.halign = egui::Align::Center;
            job.wrap.max_rows = 2;
            let galley = ctx.fonts_mut(|f| f.layout_job(job));
            assert_eq!(galley.rows.len(), 2);
            paint_lyric(
                &ctx.layer_painter(egui::LayerId::background())
                    .with_clip_rect(stage),
                stage,
                stage.top(),
                galley,
            );
        });
        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .unwrap();
        let bounds = text.galley.rect.translate(text.pos.to_vec2());
        assert!(stage.contains_rect(bounds));
        assert!((bounds.center().x - stage.center().x).abs() < 0.01);
    }

    #[test]
    fn geometry_fits_and_does_not_overlap() {
        for (w, h) in [
            (1920.0, 1080.0),
            (1280.0, 720.0),
            (800.0, 600.0),
            (480.0, 640.0),
        ] {
            let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
            for aspect in [1.0, 16.0 / 9.0, 2.0 / 3.0] {
                for lyrics_h in [0.0, 110.0, 260.0] {
                    let stage = StageLayout::new(full, aspect, lyrics_h);
                    for r in [stage.art, stage.spectrum, stage.lyrics] {
                        assert!(r.min.x >= full.min.x - 0.01 && r.max.x <= full.max.x + 0.01);
                        assert!(r.min.y >= full.min.y - 0.01 && r.max.y <= full.max.y + 0.01);
                        assert!(r.max.x >= r.min.x && r.max.y >= r.min.y);
                    }
                    assert!(stage.art.max.y <= stage.spectrum.min.y + 0.01);
                    assert!(stage.spectrum.max.y <= stage.lyrics.min.y + 0.01);
                    assert!(stage.art.width() <= 360.01 && stage.art.height() <= 300.01);
                    assert!(stage.spectrum.width() <= 520.01 && stage.spectrum.height() <= 90.01);
                    if stage.art.height() > 0.01 {
                        assert!((stage.art.width() / stage.art.height() - aspect).abs() < 0.01);
                    }
                }
            }
        }
    }
}
