//! Addressed global-menu actions; never act on whichever window a popup focused.
use crate::{dispatch, request, validate_address};
use anyhow::{Result, ensure};
use serde_json::Value;
#[derive(Clone, Debug)]
pub enum Action {
    Minimize,
    Maximize,
    Fullscreen,
    Close,
    Edit(&'static str, &'static str),
    MoveTo(String),
}
fn client(address: &str) -> Result<Value> {
    validate_address(address)?;
    serde_json::from_str::<Vec<Value>>(&request("j/clients")?)?
        .into_iter()
        .find(|w| w["address"].as_str() == Some(address))
        .ok_or_else(|| anyhow::anyhow!("The window has closed"))
}
fn move_workspace(address: &str, workspace: &str) -> Result<()> {
    let workspace_lua = serde_json::to_string(workspace)?;
    dispatch(
        &format!("movetoworkspacesilent {workspace},address:{address}"),
        &format!(
            "hl.dsp.window.move({{workspace={workspace_lua}, follow=false, window=\"address:{address}\"}})"
        ),
    )
}
pub fn restore(address: &str) -> Result<()> {
    let w = client(address)?;
    if let Some(name) = w["workspace"]["name"].as_str()
        && let Some(rest) = name.strip_prefix("special:macstill-minimized-")
        && let Some((workspace, _)) = rest.split_once('-')
        && let Ok(id) = workspace.parse::<i64>()
        && id > 0
    {
        move_workspace(address, &id.to_string())?;
    }
    Ok(())
}
pub fn perform(address: &str, class: &str, action: Action) -> Result<()> {
    let w = client(address)?;
    ensure!(
        w["class"].as_str() == Some(class),
        "The target window changed"
    );
    let target = format!("window=\"address:{address}\"");
    match action {
        Action::Close => crate::close(address),
        Action::Minimize => {
            let id = w["workspace"]["id"].as_i64().unwrap_or(0);
            ensure!(id > 0, "This window cannot be minimized here");
            move_workspace(
                address,
                &format!("special:macstill-minimized-{id}-{}", &address[2..]),
            )
        }
        Action::Maximize | Action::Fullscreen => {
            crate::focus(address)?;
            if matches!(action, Action::Maximize) {
                dispatch(
                    "fullscreen 1",
                    &format!("hl.dsp.window.fullscreen({{mode=\"maximized\", {target}}})"),
                )
            } else {
                dispatch(
                    "fullscreen 0",
                    &format!("hl.dsp.window.fullscreen({{{target}}})"),
                )
            }
        }
        Action::Edit(mods, key) => {
            crate::focus(address)?;
            dispatch(
                &format!("sendshortcut {mods},{key},address:{address}"),
                &format!("hl.dsp.send_shortcut({{mods=\"{mods}\",key=\"{key}\",{target}}})"),
            )
        }
        Action::MoveTo(name) => {
            ensure!(
                crate::displays::monitors()?.iter().any(|m| m.name == name),
                "Display disconnected"
            );
            ensure!(
                name.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
                "Invalid display name"
            );
            dispatch(
                &format!("movewindow mon:{name},address:{address}"),
                &format!(
                    "hl.dsp.window.move({{monitor={}, {target}}})",
                    serde_json::to_string(&name)?
                ),
            )?;
            crate::focus(address)
        }
    }
}
