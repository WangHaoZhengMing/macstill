//! Real workspaces and window actions used by Mission Control. No window resizing or parking.
use crate::stage::{Monitor, Window};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};
pub mod cache;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct DesktopWorkspace {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub monitor: String,
}

#[derive(Clone, Default, PartialEq)]
pub struct Scene {
    pub windows: Vec<Window>,
    pub monitors: Vec<Monitor>,
    pub spaces: Vec<DesktopWorkspace>,
}

pub fn scene() -> Result<Scene> {
    let windows: Vec<Window> = serde_json::from_str(&crate::request("j/clients")?)?;
    let monitors: Vec<Monitor> = serde_json::from_str(&crate::request("j/monitors")?)?;
    let spaces: Vec<DesktopWorkspace> = serde_json::from_str(&crate::request("j/workspaces")?)?;
    Ok(assemble(windows, monitors, spaces))
}

fn assemble(
    windows: Vec<Window>,
    monitors: Vec<Monitor>,
    mut spaces: Vec<DesktopWorkspace>,
) -> Scene {
    // 过滤无效工作区
    spaces.retain(|s| s.id > 0 && !s.name.starts_with("macstill-stage-"));
    spaces.sort_by_key(|s| s.id);

    // 1. 纯函数式过滤并构建缺失的桌面列表（修复了语法错误，并去掉了外层多余的 for 循环）
    let missing: Vec<_> = monitors
        .iter()
        .filter(|m| m.workspace.id > 0 && !m.workspace.name.starts_with("macstill-stage-"))
        .filter(|m| !spaces.iter().any(|s| s.id == m.workspace.id)) // 修复：补全了 spaces 变量名
        .map(|m| DesktopWorkspace {
            id: m.workspace.id,
            name: m.workspace.name.clone(),
            monitor: m.name.clone(),
        })
        .collect(); // 此时结束对 spaces 的只读借用

    // 2. 一次性追加到原列表中
    spaces.extend(missing); // 修复：将 desktop_workspaces 改为 spaces

    // 重新排序
    spaces.sort_by_key(|s| s.id);

    Scene {
        windows: windows
            .into_iter()
            .filter(|w| {
                w.mapped
                    && !w.hidden
                    && w.size[0] > 0
                    && w.size[1] > 0
                    && w.class != "org.macstill.Desktop"
            })
            .collect(),
        monitors,
        spaces,
    }
}

pub fn switch(id: i64) -> Result<()> {
    ensure!(id > 0, "Invalid desktop");
    crate::dispatch(
        &format!("workspace {id}"),
        &format!("hl.dsp.focus({{workspace={id}}})"),
    )
}

pub fn adjacent(step: i32) -> Result<()> {
    let s = scene()?;
    let m = s
        .monitors
        .iter()
        .find(|m| m.focused)
        .context("No focused display")?;
    let spaces: Vec<_> = s
        .spaces
        .iter()
        .filter(|s| s.monitor == m.name || s.id == m.workspace.id)
        .collect();
    let index = spaces
        .iter()
        .position(|s| s.id == m.workspace.id)
        .unwrap_or(0);
    let next = (index as i32 + step).clamp(0, spaces.len().saturating_sub(1) as i32) as usize;
    if let Some(s) = spaces.get(next) {
        switch(s.id)?;
    }
    Ok(())
}

pub fn move_window(window: &Window, id: i64) -> Result<()> {
    ensure!(id > 0, "Invalid destination desktop");
    crate::validate_address(&window.address)?;
    let live = scene()?
        .windows
        .into_iter()
        .find(|w| w.address == window.address && w.pid == window.pid && w.class == window.class)
        .context("Window has closed")?;
    let address = live.address;
    crate::dispatch(
        &format!("movetoworkspacesilent {id},address:{address}"),
        &format!(
            "hl.dsp.window.move({{workspace={id},follow=false,window=\"address:{address}\"}})"
        ),
    )
}

pub fn activate(window: &Window) -> Result<()> {
    let live = scene()?
        .windows
        .into_iter()
        .find(|w| w.address == window.address && w.pid == window.pid && w.class == window.class)
        .context("Window has closed")?;
    if live.workspace.id > 0 {
        switch(live.workspace.id)?;
    }
    crate::focus(&live.address)
}

fn directory() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("macstill")
}

#[derive(Default, Serialize, Deserialize)]
struct Owned {
    ids: BTreeSet<i64>,
}

fn owned() -> Owned {
    std::fs::read(directory().join("mission-spaces.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn persist(id: i64, enabled: bool) -> Result<()> {
    let key = format!("_macstill_space_{id}");
    let lua = format!(
        "eval if {key} then {key}:set_enabled(false) end; {key}=hl.workspace_rule({{workspace=\"{id}\",persistent={enabled}}})"
    );
    let answer = crate::request(&lua)?;
    if answer.trim() != "ok" {
        let legacy = crate::request(&format!(
            "keyword workspace {id},persistent:{}",
            if enabled { "true" } else { "false" }
        ))?;
        ensure!(
            legacy.trim() == "ok",
            "Cannot configure desktop persistence: {answer}; {legacy}"
        );
    }
    let mut owned = owned();
    if enabled {
        owned.ids.insert(id);
    } else {
        owned.ids.remove(&id);
    }
    let directory = directory();
    std::fs::create_dir_all(&directory)?;
    let mut lua = String::from("-- Desktops created by Mission Control.\n");
    for id in &owned.ids {
        if *id > 0 {
            lua.push_str(&format!(
                "_macstill_space_{id}=hl.workspace_rule({{workspace=\"{id}\",persistent=true}})\n"
            ));
        }
    }
    for (name, bytes) in [
        ("mission-spaces.json", serde_json::to_vec(&owned)?),
        ("mission-spaces.lua", lua.into_bytes()),
    ] {
        let temp = directory.join(format!("{name}.tmp"));
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, directory.join(name))?;
    }
    Ok(())
}

pub fn next_id(scene: &Scene) -> Result<i64> {
    let id = scene
        .spaces
        .iter()
        .map(|s| s.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .context("No desktop IDs available")?;
    ensure!(id > 0 && id < i32::MAX as i64, "No desktop IDs available");
    Ok(id)
}

pub fn create() -> Result<i64> {
    let s = scene()?;
    ensure!(s.spaces.len() < 32, "Maximum of 32 desktops reached");
    let id = next_id(&s)?;
    persist(id, true)?;
    switch(id)?;
    Ok(id)
}

pub fn remove(id: i64, destination: i64) -> Result<()> {
    ensure!(
        id > 0 && destination > 0 && id != destination,
        "Keep at least one desktop"
    );
    let s = scene()?;
    ensure!(
        s.spaces.iter().any(|s| s.id == destination),
        "Destination desktop no longer exists"
    );
    // Move each window before removing persistence; on failure the source remains available.
    for w in s.windows.iter().filter(|w| w.workspace.id == id) {
        move_window(w, destination)?;
    }
    if s.monitors.iter().any(|m| m.workspace.id == id) {
        switch(destination)?;
    }
    persist(id, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_space_never_reuses_a_live_id() {
        let s = Scene {
            spaces: vec![
                DesktopWorkspace {
                    id: 1,
                    name: "1".into(),
                    monitor: "A".into(),
                },
                DesktopWorkspace {
                    id: 7,
                    name: "7".into(),
                    monitor: "A".into(),
                },
            ],
            ..Scene::default()
        };
        assert_eq!(next_id(&s).unwrap(), 8);
    }
}

pub fn focus_output(output: &str) -> Result<()> {
    ensure!(
        scene()?.monitors.iter().any(|m| m.name == output),
        "Display disconnected"
    );
    let quoted = serde_json::to_string(output)?;
    crate::dispatch(
        &format!("focusmonitor {output}"),
        &format!("hl.dsp.focus({{monitor={quoted}}})"),
    )
}
