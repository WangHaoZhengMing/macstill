use crate::model::Rect;
use anyhow::{Context, Result, ensure};
use image::{ImageFormat, RgbaImage, imageops};
use std::{
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub struct Screen {
    pub name: String,
    pub rect: Rect,
    pub pixels: Arc<RgbaImage>,
    pub focused: bool,
}
#[derive(Clone)]
pub struct Target {
    pub address: String,
    pub title: String,
    pub rect: Rect,
}
pub struct Desktop {
    pub screens: Vec<Screen>,
    pub windows: Vec<Target>,
}
impl Desktop {
    pub fn load(cursor: bool) -> Result<Self> {
        let monitors: serde_json::Value =
            serde_json::from_str(&macstill_hyprland::request("j/monitors")?)?;
        let mut screens = Vec::new();
        for monitor in monitors.as_array().context("No displays")? {
            let m: macstill_hyprland::displays::Monitor = serde_json::from_value(monitor.clone())?;
            let (w, h) = m.logical_size();
            let pixels = macstill_capture::capture_output(&m.name, cursor, Duration::from_secs(3))
                .with_context(|| format!("Cannot capture {}", m.name))?;
            screens.push(Screen {
                name: m.name,
                rect: Rect {
                    x: m.x as f64,
                    y: m.y as f64,
                    w: w as f64,
                    h: h as f64,
                },
                pixels: Arc::new(pixels),
                focused: monitor["focused"].as_bool().unwrap_or(false),
            });
        }
        ensure!(!screens.is_empty(), "No connected displays");
        let active: Vec<_> = monitors
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| {
                [
                    m["activeWorkspace"]["id"].as_i64(),
                    m["specialWorkspace"]["id"].as_i64().filter(|id| *id != 0),
                ]
            })
            .flatten()
            .collect();
        let clients: serde_json::Value =
            serde_json::from_str(&macstill_hyprland::request("j/clients")?)?;
        let mut clients = clients.as_array().cloned().unwrap_or_default();
        clients.sort_by_key(|c| {
            c["focusHistoryID"]
                .as_i64()
                .filter(|n| *n >= 0)
                .unwrap_or(i64::MAX)
        });
        let windows = clients
            .iter()
            .filter(|c| {
                c["mapped"].as_bool() != Some(false)
                    && c["hidden"].as_bool() != Some(true)
                    && c["workspace"]["id"]
                        .as_i64()
                        .is_some_and(|id| active.contains(&id))
            })
            .filter_map(|c| {
                Some(Target {
                    address: c["address"].as_str()?.into(),
                    title: c["title"].as_str().unwrap_or("Window").into(),
                    rect: Rect {
                        x: c["at"][0].as_f64()?,
                        y: c["at"][1].as_f64()?,
                        w: c["size"][0].as_f64()?,
                        h: c["size"][1].as_f64()?,
                    },
                })
            })
            .collect();
        Ok(Self { screens, windows })
    }
    pub fn crop(&self, area: Rect) -> Result<RgbaImage> {
        let scale = self
            .screens
            .iter()
            .filter(|s| s.rect.intersection(area).is_some())
            .map(|s| s.pixels.width() as f64 / s.rect.w)
            .fold(0., f64::max);
        let (w, h) = area.pixels(scale)?;
        let mut output = RgbaImage::new(w, h);
        for screen in &self.screens {
            let Some(part) = screen.rect.intersection(area) else {
                continue;
            };
            let sx = screen.pixels.width() as f64 / screen.rect.w;
            let sy = screen.pixels.height() as f64 / screen.rect.h;
            let x = ((part.x - screen.rect.x) * sx).round() as u32;
            let y = ((part.y - screen.rect.y) * sy).round() as u32;
            let right = (((part.x + part.w - screen.rect.x) * sx).round() as u32)
                .min(screen.pixels.width());
            let bottom = (((part.y + part.h - screen.rect.y) * sy).round() as u32)
                .min(screen.pixels.height());
            if right <= x || bottom <= y {
                continue;
            }
            let piece =
                imageops::crop_imm(screen.pixels.as_ref(), x, y, right - x, bottom - y).to_image();
            // Derive both edges from the same global transform to avoid fractional-scale seams.
            let dx = ((part.x - area.x) * scale).round() as u32;
            let dy = ((part.y - area.y) * scale).round() as u32;
            let dw = ((part.x + part.w - area.x) * scale).round() as u32 - dx;
            let dh = ((part.y + part.h - area.y) * scale).round() as u32 - dy;
            let piece = imageops::resize(&piece, dw, dh, imageops::FilterType::Lanczos3);
            imageops::overlay(&mut output, &piece, dx as i64, dy as i64);
        }
        Ok(output)
    }
}
pub fn png(pixels: &RgbaImage) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    pixels.write_to(&mut bytes, ImageFormat::Png)?;
    Ok(bytes.into_inner())
}
pub fn save(bytes: &[u8], directory: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("Cannot create {}", directory.display()))?;
    let base = format!(
        "Screenshot {}",
        chrono::Local::now().format("%Y-%m-%d at %H.%M.%S")
    );
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    for n in 0..10000 {
        let path = directory.join(if n == 0 {
            format!("{base}.png")
        } else {
            format!("{base} ({n}).png")
        });
        match temp.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temp = e.file,
            Err(e) => return Err(e.error.into()),
        }
    }
    anyhow::bail!("Too many screenshots with the same timestamp")
}
pub fn clipboard(bytes: &[u8]) -> Result<()> {
    // wl-copy keeps serving image/png after this app exits; GPUI's clipboard is tied to its lifetime.
    // Spool to an anonymous file so a stalled clipboard helper cannot block a pipe writer.
    let mut input = tempfile::tempfile()?;
    input.write_all(bytes)?;
    use std::io::{Seek, SeekFrom};
    input.seek(SeekFrom::Start(0))?;
    let mut child = std::process::Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(input)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Install wl-clipboard to copy screenshots")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "Clipboard service rejected the image");
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Clipboard service timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_scale_negative_origin_crop_has_exact_edges() {
        let desktop = Desktop {
            windows: vec![],
            screens: vec![
                Screen {
                    name: "left".into(),
                    rect: Rect {
                        x: -100.,
                        y: 0.,
                        w: 100.,
                        h: 100.,
                    },
                    pixels: Arc::new(RgbaImage::from_pixel(
                        125,
                        125,
                        image::Rgba([255, 0, 0, 255]),
                    )),
                    focused: false,
                },
                Screen {
                    name: "right".into(),
                    rect: Rect {
                        x: 0.,
                        y: 0.,
                        w: 100.,
                        h: 100.,
                    },
                    pixels: Arc::new(RgbaImage::from_pixel(
                        200,
                        200,
                        image::Rgba([0, 255, 0, 255]),
                    )),
                    focused: true,
                },
            ],
        };
        let image = desktop
            .crop(Rect {
                x: -50.,
                y: 10.,
                w: 100.,
                h: 50.,
            })
            .unwrap();
        assert_eq!(image.dimensions(), (200, 100));
        assert_eq!(image.get_pixel(99, 99).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(100, 99).0, [0, 255, 0, 255]);
        assert_eq!(image.get_pixel(199, 99).0, [0, 255, 0, 255]);
        assert!(
            desktop
                .crop(Rect {
                    x: 999.,
                    y: 999.,
                    w: 10.,
                    h: 10.
                })
                .is_err()
        );
    }
    #[test]
    fn saving_never_overwrites_and_png_roundtrips() {
        let d = tempfile::tempdir().unwrap();
        let bytes = png(&RgbaImage::from_pixel(5, 7, image::Rgba([12, 34, 56, 255]))).unwrap();
        let a = save(&bytes, d.path()).unwrap();
        let b = save(&bytes, d.path()).unwrap();
        assert_ne!(a, b);
        assert_eq!(
            image::open(a).unwrap().to_rgba8().get_pixel(4, 6).0,
            [12, 34, 56, 255]
        );
        assert_eq!(std::fs::read(b).unwrap(), bytes);
    }
}
