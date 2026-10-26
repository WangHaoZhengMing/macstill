use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    Screen,
    Window,
    #[default]
    Region,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub mode: Mode,
    pub immediate: bool,
    pub clipboard: bool,
    pub output: Option<PathBuf>,
}
impl Request {
    /// Launch flags override only this invocation; saved Options remain intact.
    pub fn with_preferences(mut self, prefs: &Preferences) -> Self {
        if !self.immediate {
            self.mode = prefs.mode;
        }
        if self.output.is_none() {
            self.clipboard |= prefs.clipboard;
        }
        self
    }

    pub fn parse(args: &[String]) -> Result<Self> {
        let mut result = Self::default();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--fullscreen" => {
                    result.mode = Mode::Screen;
                    result.immediate = true;
                }
                "--region" => {
                    result.mode = Mode::Region;
                    result.immediate = true;
                }
                "--window" => {
                    result.mode = Mode::Window;
                    result.immediate = true;
                }
                "--clipboard" => result.clipboard = true,
                "--output" => {
                    result.output = Some(
                        args.next()
                            .ok_or_else(|| anyhow::anyhow!("--output needs a directory"))?
                            .into(),
                    )
                }
                _ => bail!("Unknown option: {arg}"),
            }
        }
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn between(a: (f64, f64), b: (f64, f64)) -> Self {
        Self {
            x: a.0.min(b.0),
            y: a.1.min(b.1),
            w: (a.0 - b.0).abs(),
            h: (a.1 - b.1).abs(),
        }
    }
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn intersection(self, b: Self) -> Option<Self> {
        let x = self.x.max(b.x);
        let y = self.y.max(b.y);
        let w = (self.x + self.w).min(b.x + b.w) - x;
        let h = (self.y + self.h).min(b.y + b.h) - y;
        (w > 0. && h > 0.).then_some(Self { x, y, w, h })
    }
    pub fn pixels(self, scale: f64) -> Result<(u32, u32)> {
        ensure!(
            [self.x, self.y, self.w, self.h, scale]
                .iter()
                .all(|v| v.is_finite())
                && self.w >= 1.
                && self.h >= 1.
                && scale > 0.,
            "Select an area first"
        );
        let w = (self.w * scale).round();
        let h = (self.h * scale).round();
        ensure!(
            w >= 1. && h >= 1. && w * h <= 64. * 1024. * 1024.,
            "Screenshot exceeds 64 megapixels"
        );
        Ok((w as u32, h as u32))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub directory: PathBuf,
    pub delay: u64,
    pub cursor: bool,
    pub thumbnail: bool,
    pub mode: Mode,
    pub region: Option<Rect>,
    pub remember: bool,
    pub clipboard: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        let directory = std::process::Command::new("xdg-user-dir")
            .arg("DESKTOP")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Desktop")
            });
        Self {
            directory,
            delay: 0,
            cursor: false,
            thumbnail: true,
            mode: Mode::Region,
            region: None,
            remember: true,
            clipboard: false,
        }
    }
}
impl Preferences {
    fn path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            })
            .join("macstill/screenshot.json")
    }
    pub fn load() -> Self {
        Self::load_at(&Self::path())
    }
    fn load_at(path: &std::path::Path) -> Self {
        let mut p: Self = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if ![0, 5, 10].contains(&p.delay) {
            p.delay = 0;
        }
        p
    }
    pub fn save(&self) -> Result<()> {
        self.save_at(&Self::path())
    }
    fn save_at(&self, path: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut f = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        serde_json::to_writer_pretty(&mut f, self)?;
        f.persist(path)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_survive_reopening_without_taking_a_screenshot() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("screenshot.json");
        let prefs = Preferences {
            directory: folder.path().join("Custom Screenshots"),
            delay: 10,
            cursor: true,
            thumbnail: false,
            remember: false,
            clipboard: true,
            mode: Mode::Window,
            ..Preferences::default()
        };
        prefs.save_at(&path).unwrap();
        let restored = Preferences::load_at(&path);
        assert_eq!(restored.directory, prefs.directory);
        assert_eq!(restored.delay, 10);
        assert!(restored.cursor && restored.clipboard);
        assert!(!restored.thumbnail && !restored.remember);
        for args in [vec![], vec!["--region".into()], vec!["--fullscreen".into()]] {
            let request = Request::parse(&args).unwrap().with_preferences(&restored);
            assert!(request.clipboard);
            if args.is_empty() {
                assert_eq!(request.mode, Mode::Window);
            } else if args[0] == "--region" {
                assert_eq!(request.mode, Mode::Region);
            } else {
                assert_eq!(request.mode, Mode::Screen);
            }
        }
    }
    #[test]
    fn launch_overrides_do_not_replace_saved_options() {
        let prefs = Preferences {
            clipboard: false,
            ..Preferences::default()
        };
        let copy = Request::parse(&["--region".into(), "--clipboard".into()])
            .unwrap()
            .with_preferences(&prefs);
        assert!(copy.clipboard);
        assert!(!Request::default().with_preferences(&prefs).clipboard);
        let prefs = Preferences {
            clipboard: true,
            ..prefs
        };
        let file = Request::parse(&["--output".into(), "/tmp/screenshots".into()])
            .unwrap()
            .with_preferences(&prefs);
        assert!(!file.clipboard);
        assert_eq!(
            file.output.as_deref(),
            Some(std::path::Path::new("/tmp/screenshots"))
        );
        assert!(Request::default().with_preferences(&prefs).clipboard);
    }
    #[test]
    fn reverse_drag_negative_origin_and_fractional_size() {
        let r = Rect::between((100., 200.), (-400., -100.));
        assert_eq!(
            r,
            Rect {
                x: -400.,
                y: -100.,
                w: 500.,
                h: 300.
            }
        );
        assert_eq!(r.pixels(1.25).unwrap(), (625, 375));
        assert_eq!(
            r.intersection(Rect {
                x: 0.,
                y: 0.,
                w: 1000.,
                h: 1000.
            })
            .unwrap(),
            Rect {
                x: 0.,
                y: 0.,
                w: 100.,
                h: 200.
            }
        );
        assert!(Rect::default().pixels(1.).is_err());
        assert!(
            Rect {
                x: 0.,
                y: 0.,
                w: 1e9,
                h: 1e9
            }
            .pixels(2.)
            .is_err()
        );
    }
    #[test]
    fn command_options() {
        let r = Request::parse(&["--window".into(), "--clipboard".into()]).unwrap();
        assert_eq!(r.mode, Mode::Window);
        assert!(r.clipboard && r.immediate);
        assert!(Request::parse(&["--output".into()]).is_err());
        assert!(Request::parse(&["--typo".into()]).is_err());
    }
}

#[derive(Clone, Copy)]
pub struct Drag {
    start: (f64, f64),
    original: Option<Rect>,
    edges: [bool; 4],
}
impl Drag {
    pub fn new(start: (f64, f64), area: Option<Rect>) -> Self {
        let mut edges = [false; 4];
        let original = area.filter(|r| {
            let near = Rect {
                x: r.x - 7.,
                y: r.y - 7.,
                w: r.w + 14.,
                h: r.h + 14.,
            }
            .contains(start.0, start.1);
            if near {
                edges = [
                    (start.0 - r.x).abs() < 7.,
                    (start.0 - r.x - r.w).abs() < 7.,
                    (start.1 - r.y).abs() < 7.,
                    (start.1 - r.y - r.h).abs() < 7.,
                ];
            }
            near
        });
        Self {
            start,
            original,
            edges,
        }
    }
    pub fn area(self, point: (f64, f64)) -> Rect {
        let Some(r) = self.original else {
            return Rect::between(self.start, point);
        };
        let dx = point.0 - self.start.0;
        let dy = point.1 - self.start.1;
        if !self.edges.contains(&true) {
            return Rect {
                x: r.x + dx,
                y: r.y + dy,
                ..r
            };
        }
        Rect::between(
            (
                r.x + if self.edges[0] { dx } else { 0. },
                r.y + if self.edges[2] { dy } else { 0. },
            ),
            (
                r.x + r.w + if self.edges[1] { dx } else { 0. },
                r.y + r.h + if self.edges[3] { dy } else { 0. },
            ),
        )
    }
}
