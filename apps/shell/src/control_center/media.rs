use super::*;
use std::process::Command;
// Cover retrieval and decoding run on the status worker, never in render().
pub(super) fn media_cover(url: &str) -> Option<Vec<u8>> {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
        time::Instant,
    };
    type Covers = HashMap<String, (Instant, Option<Vec<u8>>)>;
    static CACHE: OnceLock<Mutex<Covers>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((time, bytes)) = cache.lock().unwrap().get(url) {
        if time.elapsed() < Duration::from_secs(60) {
            return bytes.clone();
        }
    }
    let load = || -> Option<Vec<u8>> {
        let bytes = if url.starts_with("file://") {
            let file = gio::File::for_uri(url);
            let path = gio::prelude::FileExt::path(&file)?;
            if std::fs::metadata(&path).ok()?.len() > 8 * 1024 * 1024 {
                return None;
            }
            std::fs::read(path).ok()?
        } else if url.starts_with("https://") || url.starts_with("http://") {
            let out = Command::new("curl")
                .args([
                    "--fail",
                    "--silent",
                    "--location",
                    "--max-time",
                    "3",
                    "--max-filesize",
                    "8388608",
                    "--proto",
                    "=http,https",
                    "--proto-redir",
                    "=http,https",
                    "--",
                    url,
                ])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            out.stdout
        } else {
            return None;
        };
        if bytes.len() > 8 * 1024 * 1024 {
            return None;
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .ok()?;
        let (w, h) = reader.into_dimensions().ok()?;
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return None;
        }
        let image = image::load_from_memory(&bytes).ok()?.thumbnail(128, 128);
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).ok()?;
        Some(png.into_inner())
    };
    let bytes = load();
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 32 {
        cache.clear()
    }
    cache.insert(url.into(), (Instant::now(), bytes.clone()));
    bytes
}
impl DesktopView {
    pub(super) fn media_card(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        use macstill_system::media::Command as MediaCommand;
        let state = &self.control_center.status;
        let player = state.media.as_ref();
        let title = player
            .map(|p| {
                if p.title.is_empty() {
                    p.identity.clone()
                } else {
                    p.title.clone()
                }
            })
            .unwrap_or_else(|| "Music".into());
        let detail = player
            .map(|p| {
                if !p.artists.is_empty() {
                    p.artists.join(", ")
                } else if !p.album.is_empty() {
                    p.album.clone()
                } else {
                    format!("{} · {}", p.identity, p.status)
                }
            })
            .unwrap_or_default();
        let cover = state
            .cover
            .as_ref()
            .map(|bytes| {
                img(std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    bytes.clone(),
                )))
                .size(px(46.))
                .rounded(px(4.))
                .object_fit(ObjectFit::Cover)
                .into_any_element()
            })
            .unwrap_or_else(|| {
                div()
                    .size(px(46.))
                    .rounded(px(4.))
                    .bg(if p.dark {
                        rgba(0x00000018)
                    } else {
                        rgba(0x69739630)
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::cc_icon("music", 20., p.text, p.dark))
                    .into_any_element()
            });
        let mut card = control_center_tile(p)
            .h(px(68.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(cover)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().text_size(px(11.)).truncate().child(title))
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(rgb(p.muted))
                            .truncate()
                            .child(detail),
                    ),
            );
        for (id, icon, command, enabled) in [
            (
                "cc-play",
                if player.is_some_and(|p| p.playing()) {
                    "pause"
                } else {
                    "play"
                },
                MediaCommand::PlayPause,
                player.is_some_and(|p| p.can_toggle()),
            ),
            (
                "cc-next",
                "next",
                MediaCommand::Next,
                player.is_some_and(|p| p.can_control && p.can_next),
            ),
        ] {
            let icon = Self::cc_icon(icon, 14., p.text, p.dark);
            card = card.child(
                gpui_kit::base::Button::new(id)
                    .disabled(!enabled || self.control_center.busy)
                    .size(px(22.))
                    .cursor_pointer()
                    .child(icon)
                    .on_click(cx.listener(move |s, _, _, cx| s.media_command(command, cx))),
            );
        }
        card.into_any_element()
    }
    pub(super) fn media_command(
        &mut self,
        command: macstill_system::media::Command,
        cx: &mut Context<Self>,
    ) {
        let Some(player) = self.control_center.status.media.clone() else {
            return;
        };
        if self.control_center.busy {
            return;
        }
        self.control_center.busy = true;
        self.message = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    macstill_system::SystemApi::new()
                        .media()
                        .command(&player, command)
                })
                .await;
            let _ = this.update_in(cx, |s, w, cx| {
                s.control_center.busy = false;
                if let Err(e) = result {
                    s.message = Some(e.to_string())
                }
                s.refresh_control_center(w, cx);
                cx.notify();
            });
        })
        .detach();
    }
}
