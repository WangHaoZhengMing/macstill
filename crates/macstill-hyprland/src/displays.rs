//! Per-user output scaling, applied to the compositor rather than shell widgets.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Monitor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    #[serde(rename = "refreshRate")]
    pub refresh_rate: f64,
    #[serde(default)]
    pub transform: u32,
}
pub fn monitors() -> Result<Vec<Monitor>> {
    Ok(serde_json::from_str(&crate::request("j/monitors")?)?)
}
impl Monitor {
    pub fn logical_size(&self) -> (u32, u32) {
        let (w, h) = if self.transform % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        (
            (w as f64 / self.scale).round() as u32,
            (h as f64 / self.scale).round() as u32,
        )
    }
    pub fn scales(&self) -> Vec<f64> {
        [1., 1.25, 1.5, 1.75, 2., 2.5, 3.]
            .into_iter()
            .filter(|s| {
                let w = self.width as f64 / s;
                let h = self.height as f64 / s;
                w >= 640.
                    && h >= 360.
                    && (w - w.round()).abs() < 0.001
                    && (h - h.round()).abs() < 0.001
            })
            .collect()
    }
    fn rule(&self) -> Result<String> {
        ensure!(
            !self.name.is_empty()
                && self
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.:".contains(c)),
            "Unsupported display connector name"
        );
        ensure!(
            self.scale.is_finite() && self.scale >= 0.5 && self.scale <= 4.,
            "Invalid output scale"
        );
        ensure!(
            self.width > 0
                && self.height > 0
                && self.refresh_rate.is_finite()
                && self.refresh_rate > 0.,
            "Invalid display mode"
        );
        Ok(format!(
            "hl.monitor({{output=\"{}\",mode=\"{}x{}@{:.5}\",position=\"{}x{}\",scale={},transform={}}})",
            self.name,
            self.width,
            self.height,
            self.refresh_rate,
            self.x,
            self.y,
            self.scale,
            self.transform
        ))
    }
}
fn directory() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("macstill")
}
fn apply_rule(m: &Monitor) -> Result<()> {
    let lua = m.rule()?;
    let response = crate::request(&format!("eval {lua}"))?;
    if response.trim() == "ok" {
        return Ok(());
    }
    if response.contains("unknown request")
        || response.contains("Unknown request")
        || response.contains("not support")
    {
        let legacy = format!(
            "keyword monitor {},{}x{}@{:.5},{}x{},{},transform,{}",
            m.name, m.width, m.height, m.refresh_rate, m.x, m.y, m.scale, m.transform
        );
        let response = crate::request(&legacy)?;
        ensure!(
            response.trim() == "ok",
            "Hyprland rejected scaling: {response}"
        );
        return Ok(());
    }
    bail!("Hyprland rejected scaling: {response}")
}
/// Saves only after the compositor confirms the new scale. Failures roll back.
pub fn set_scale(name: &str, scale: f64) -> Result<()> {
    let current = monitors()?
        .into_iter()
        .find(|m| m.name == name)
        .context("This display is no longer connected")?;
    ensure!(
        scale.is_finite() && current.scales().contains(&scale),
        "This scale is not compatible with the display resolution"
    );
    let dir = directory();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("display-scales.json");
    let mut saved: BTreeMap<String, Monitor> = if path.exists() {
        serde_json::from_slice(&std::fs::read(&path)?)?
    } else {
        BTreeMap::new()
    };
    let lua_path = dir.join("display-scales.lua");
    let previous_lua = if lua_path.exists() {
        Some(std::fs::read(&lua_path)?)
    } else {
        None
    };
    let previous_json = if path.exists() {
        Some(std::fs::read(&path)?)
    } else {
        None
    };
    let mut next = current.clone();
    next.scale = scale;
    apply_rule(&next)?;
    let mut accepted = false;
    for _ in 0..20 {
        if monitors().is_ok_and(|list| {
            list.into_iter()
                .any(|m| m.name == name && (m.scale - scale).abs() < 0.01)
        }) {
            accepted = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !accepted {
        let _ = apply_rule(&current);
        bail!("The display did not accept this scale")
    }
    saved.insert(name.into(), next);
    let save = || -> Result<()> {
        let mut lua =
            String::from("-- Generated by Macstill Display Settings. Per-user output scaling.\n");
        for m in saved.values() {
            lua.push_str(&m.rule()?);
            lua.push('\n');
        }
        std::fs::write(dir.join("display-scales.lua.tmp"), lua)?;
        std::fs::rename(
            dir.join("display-scales.lua.tmp"),
            dir.join("display-scales.lua"),
        )?;
        std::fs::write(
            dir.join("display-scales.json.tmp"),
            serde_json::to_vec_pretty(&saved)?,
        )?;
        std::fs::rename(dir.join("display-scales.json.tmp"), &path)?;
        Ok(())
    };
    if let Err(e) = save() {
        let _ = apply_rule(&current);
        for (file, original) in [(&lua_path, previous_lua), (&path, previous_json)] {
            if let Some(bytes) = original {
                let _ = std::fs::write(file, bytes);
            } else {
                let _ = std::fs::remove_file(file);
            }
        }
        return Err(e.context("Could not save display scaling"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn monitor() -> Monitor {
        Monitor {
            name: "DP-1".into(),
            description: String::new(),
            width: 1920,
            height: 1080,
            x: -1920,
            y: 0,
            scale: 1.,
            refresh_rate: 60.,
            transform: 0,
        }
    }
    #[test]
    fn only_integral_logical_sizes_are_offered() {
        let m = monitor();
        assert!(m.scales().contains(&1.5));
        assert!(!m.scales().contains(&1.75));
    }
    #[test]
    fn keeps_mode_position_and_transform() {
        let mut m = monitor();
        m.scale = 1.5;
        m.transform = 1;
        let rule = m.rule().unwrap();
        assert!(rule.contains("1920x1080@60.00000"));
        assert!(rule.contains("-1920x0"));
        assert!(rule.contains("transform=1"));
    }
    #[test]
    fn rejects_nonfinite_scale_and_injected_connector() {
        let mut m = monitor();
        m.scale = f64::NAN;
        assert!(m.rule().is_err());
        m.scale = 1.;
        m.name = "DP-1\"});os.exit()".into();
        assert!(m.rule().is_err());
    }
}

/// Validate an extended desktop: no overlaps and every output has a shared edge.
pub fn validate_arrangement(monitors: &[Monitor]) -> Result<()> {
    ensure!(!monitors.is_empty(), "No displays connected");
    let mut reached = vec![false; monitors.len()];
    reached[0] = true;
    for (i, a) in monitors.iter().enumerate() {
        a.rule()?;
        ensure!(
            a.x.abs_diff(0) <= 100_000 && a.y.abs_diff(0) <= 100_000,
            "Display position is too far away"
        );
        let (aw, ah) = a.logical_size();
        for b in &monitors[i + 1..] {
            ensure!(a.name != b.name, "Duplicate display");
            let (bw, bh) = b.logical_size();
            ensure!(
                a.x as i64 + aw as i64 <= b.x as i64
                    || b.x as i64 + bw as i64 <= a.x as i64
                    || a.y as i64 + ah as i64 <= b.y as i64
                    || b.y as i64 + bh as i64 <= a.y as i64,
                "Displays must not overlap"
            );
        }
    }
    for _ in 0..monitors.len() {
        for (i, a) in monitors.iter().enumerate() {
            for (j, b) in monitors.iter().enumerate() {
                let (aw, ah) = a.logical_size();
                let (bw, bh) = b.logical_size();
                let vertical = (a.x as i64 + aw as i64 == b.x as i64
                    || b.x as i64 + bw as i64 == a.x as i64)
                    && (a.y as i64) < b.y as i64 + bh as i64
                    && (b.y as i64) < a.y as i64 + ah as i64;
                let horizontal = (a.y as i64 + ah as i64 == b.y as i64
                    || b.y as i64 + bh as i64 == a.y as i64)
                    && (a.x as i64) < b.x as i64 + bw as i64
                    && (b.x as i64) < a.x as i64 + aw as i64;
                if reached[j] && (vertical || horizontal) {
                    reached[i] = true;
                }
            }
        }
    }
    ensure!(
        reached.iter().all(|v| *v),
        "Place displays edge to edge so the pointer can move between them"
    );
    Ok(())
}

pub fn set_arrangement(positions: &[(String, i32, i32)]) -> Result<()> {
    let current = monitors()?;
    ensure!(
        positions.len() == current.len(),
        "Connected displays changed; reopen Arrange"
    );
    let mut next = current.clone();
    for m in &mut next {
        let matches: Vec<_> = positions
            .iter()
            .filter(|(name, _, _)| name == &m.name)
            .collect();
        ensure!(
            matches.len() == 1,
            "Connected displays changed; reopen Arrange"
        );
        m.x = matches[0].1;
        m.y = matches[0].2;
    }
    validate_arrangement(&next)?;
    let dir = directory();
    std::fs::create_dir_all(&dir)?;
    let json_path = dir.join("display-scales.json");
    let lua_path = dir.join("display-scales.lua");
    let read = |path: &PathBuf| -> Result<Option<Vec<u8>>> {
        match std::fs::read(path) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    };
    let old_json = read(&json_path)?;
    let old_lua = read(&lua_path)?;
    let mut saved: BTreeMap<String, Monitor> = old_json
        .as_ref()
        .map(|b| serde_json::from_slice(b))
        .transpose()?
        .unwrap_or_default();
    for m in &next {
        saved.insert(m.name.clone(), m.clone());
    }
    let mut lua = String::from("-- Generated by Macstill Display Settings.\n");
    for m in saved.values() {
        lua.push_str(&m.rule()?);
        lua.push('\n');
    }
    let result = (|| -> Result<()> {
        for m in &next {
            apply_rule(m)?;
        }
        let mut accepted = false;
        for _ in 0..20 {
            if monitors().is_ok_and(|actual| {
                actual.len() == next.len()
                    && next.iter().all(|n| {
                        actual
                            .iter()
                            .any(|a| a.name == n.name && a.x == n.x && a.y == n.y)
                    })
            }) {
                accepted = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        ensure!(accepted, "The compositor did not accept this arrangement");
        std::fs::write(lua_path.with_extension("lua.tmp"), &lua)?;
        std::fs::write(
            json_path.with_extension("json.tmp"),
            serde_json::to_vec_pretty(&saved)?,
        )?;
        std::fs::rename(lua_path.with_extension("lua.tmp"), &lua_path)?;
        std::fs::rename(json_path.with_extension("json.tmp"), &json_path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        for m in &current {
            if let Err(e) = apply_rule(m) {
                rollback_errors.push(format!("{}: {e}", m.name));
            }
        }
        for (path, bytes) in [(&json_path, old_json), (&lua_path, old_lua)] {
            let restored = if let Some(bytes) = bytes {
                std::fs::write(path, bytes)
            } else {
                match std::fs::remove_file(path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                }
            };
            if let Err(e) = restored {
                rollback_errors.push(format!("{}: {e}", path.display()));
            }
        }
        if !rollback_errors.is_empty() {
            return Err(error.context(format!(
                "Could not apply arrangement; restoring the previous layout also failed: {}",
                rollback_errors.join("; ")
            )));
        }
        return Err(error.context("Could not apply display arrangement; previous layout restored"));
    }
    Ok(())
}
