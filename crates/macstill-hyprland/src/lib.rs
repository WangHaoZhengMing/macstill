use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};
#[derive(Clone, Debug, Deserialize)]
pub struct Client {
    #[serde(default, rename = "focusHistoryID")]
    pub focus_history_id: Option<i64>,
    #[serde(default)]
    pub auxiliary: bool,
    #[serde(default, rename = "initialClass")]
    pub initial_class: String,
    pub address: String,
    pub class: String,
    #[serde(default)]
    pub title: String,
}
pub fn socket(name: &str) -> Result<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .context("Run the shell inside a Hyprland session, or use --preview")?;
    Ok(
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is missing")?)
            .join("hypr")
            .join(signature)
            .join(name),
    )
}
pub fn request(message: &str) -> Result<String> {
    let mut stream =
        UnixStream::connect(socket(".socket.sock")?).context("Cannot connect to Hyprland IPC")?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(message.as_bytes())?;
    let mut result = String::new();
    stream.read_to_string(&mut result)?;
    Ok(result)
}
pub fn clients() -> Result<Vec<Client>> {
    let mut clients: Vec<Client> = serde_json::from_str(&request("j/clients")?)?;
    if let Ok(reply) = request("macstill-stage relationships")
        && let Ok(relations) = serde_json::from_str::<Vec<serde_json::Value>>(&reply)
    {
        for client in &mut clients {
            client.auxiliary = relations.iter().any(|r| {
                r["address"].as_str() == Some(&client.address)
                    && (r["parent"].as_str().is_some() || r["modal"].as_bool() == Some(true))
            });
        }
    }
    Ok(clients)
}
pub fn focus(address: &str) -> Result<()> {
    window_menu::restore(address)?;
    validate_address(address)?;
    dispatch(
        &format!("focuswindow address:{address}"),
        &format!("hl.dsp.focus({{window = \"address:{address}\"}})"),
    )?;
    dispatch(
        &format!("alterzorder top,address:{address}"),
        &format!("hl.dsp.window.alter_zorder({{mode = \"top\", window = \"address:{address}\"}})"),
    )
}

/// Request a normal close without killing the process or changing keyboard focus.
pub fn close(address: &str) -> Result<()> {
    validate_address(address)?;
    dispatch(
        &format!("closewindow address:{address}"),
        &format!("hl.dsp.window.close({{window = \"address:{address}\"}})"),
    )
}

fn validate_address(address: &str) -> Result<()> {
    if address.len() <= 2
        || !address.starts_with("0x")
        || !address[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        bail!("Invalid window address")
    }
    Ok(())
}
fn dispatch(legacy: &str, lua: &str) -> Result<()> {
    dispatch_with(legacy, lua, request)
}
fn dispatch_with(
    legacy: &str,
    lua: &str,
    mut transport: impl FnMut(&str) -> Result<String>,
) -> Result<()> {
    let mut result = transport(&format!("dispatch {lua}"))?;
    // Lua-configured Hyprland evaluates dispatch expressions; hyprlang sessions
    // reject the expression before doing anything, so retry the legacy form only then.
    if result.trim() == "Invalid dispatcher" {
        result = transport(&format!("dispatch {legacy}"))?;
    }
    if result.trim() != "ok" {
        bail!("Hyprland: {result}")
    }
    Ok(())
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    #[test]
    fn lua_and_hyprlang_sessions_use_their_own_dispatch_protocol_without_double_actions() {
        let mut sent = Vec::new();
        dispatch_with(
            "fullscreen 1",
            "hl.dsp.window.fullscreen({mode='maximized'})",
            |message| {
                sent.push(message.to_owned());
                Ok(if sent.len() == 1 {
                    "Invalid dispatcher"
                } else {
                    "ok"
                }
                .into())
            },
        )
        .unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1], "dispatch fullscreen 1");
        let mut count = 0;
        assert!(
            dispatch_with("fullscreen 1", "hl.dsp.window.fullscreen({})", |_| {
                count += 1;
                Ok("error: window no longer exists".into())
            })
            .is_err()
        );
        assert_eq!(count, 1);
    }
}

mod windows;
pub use windows::{WindowCommand, manage_window};

pub mod displays;
pub mod input;
pub mod stage;

pub mod mission;

pub mod window_menu;
