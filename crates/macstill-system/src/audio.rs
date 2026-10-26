//! PipeWire's PulseAudio compatibility interface, via machine-readable pactl JSON.
//! No parsing of localized display text and no shell interpolation.
use crate::{error::*, model::*};
use serde_json::Value;
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub trait CommandRunner {
    fn run(&self, args: &[String]) -> Result<Vec<u8>>;
}
#[derive(Default)]
pub struct PactlRunner;
impl CommandRunner for PactlRunner {
    fn run(&self, args: &[String]) -> Result<Vec<u8>> {
        let mut child = Command::new("pactl")
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SystemError::io("audio", e))?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        // Drain both pipes while waiting: a large graph must not deadlock on a full pipe.
        let output = thread::spawn(move || read_bounded(stdout));
        let errors = thread::spawn(move || read_bounded(stderr));
        let deadline = Instant::now() + Duration::from_secs(3);
        let result = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(SystemError::new(
                        ErrorKind::Timeout,
                        "audio",
                        "Audio service did not respond within 3 seconds",
                    ));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(SystemError::io("audio", e));
                }
            }
        };
        let stdout = output
            .join()
            .map_err(|_| SystemError::protocol("audio reader"))?;
        let stderr = errors
            .join()
            .map_err(|_| SystemError::protocol("audio reader"))?;
        let status = result?;
        let stdout = stdout?;
        let stderr = stderr?;
        if !status.success() {
            let text = String::from_utf8_lossy(&stderr);
            let kind = if text.contains("Access denied") {
                ErrorKind::PermissionDenied
            } else if text.contains("No such entity") {
                ErrorKind::NotFound
            } else if text.contains("Connection failure") || text.contains("Connection refused") {
                ErrorKind::Unavailable
            } else {
                ErrorKind::Failed
            };
            return Err(SystemError::new(kind, "audio", text.trim()));
        }
        Ok(stdout)
    }
}
fn read_bounded(reader: impl Read) -> Result<Vec<u8>> {
    const LIMIT: u64 = 2 * 1024 * 1024;
    let mut bytes = vec![];
    reader
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SystemError::io("audio output", e))?;
    if bytes.len() as u64 > LIMIT {
        return Err(SystemError::new(
            ErrorKind::Failed,
            "audio output",
            "Audio response exceeds 2 MiB",
        ));
    }
    Ok(bytes)
}
pub struct Audio<'a, R: CommandRunner>(pub &'a R);
impl<R: CommandRunner> Audio<'_, R> {
    fn json(&self, args: &[&str]) -> Result<Value> {
        let mut a = vec!["--format=json".to_owned()];
        a.extend(args.iter().map(|s| (*s).to_owned()));
        serde_json::from_slice(&self.0.run(&a)?)
            .map_err(|_| SystemError::protocol("pactl JSON; requires pactl 16+"))
    }
    pub fn snapshot(&self) -> Result<AudioState> {
        let info = self.json(&["info"])?;
        let sink = info["default_sink_name"].as_str().unwrap_or_default();
        let source = info["default_source_name"].as_str().unwrap_or_default();
        Ok(AudioState {
            server: info["server_name"].as_str().unwrap_or_default().into(),
            outputs: self.devices(AudioDirection::Output, sink)?,
            inputs: self.devices(AudioDirection::Input, source)?,
        })
    }
    fn devices(&self, direction: AudioDirection, default: &str) -> Result<Vec<AudioDevice>> {
        let kind = match direction {
            AudioDirection::Output => "sinks",
            AudioDirection::Input => "sources",
        };
        let array = self.json(&["list", kind])?;
        let mut result = vec![];
        for device in array
            .as_array()
            .ok_or_else(|| SystemError::protocol("audio devices"))?
        {
            // Loopback monitor sources are not microphones.
            if direction == AudioDirection::Input
                && (device["monitor_source"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
                    || device["monitor_of_sink"]
                        .as_u64()
                        .is_some_and(|id| id != u32::MAX as u64))
            {
                continue;
            }
            let name = device["name"]
                .as_str()
                .ok_or_else(|| SystemError::protocol("audio device name"))?;
            let channel_map = device["channel_map"]
                .as_str()
                .ok_or_else(|| SystemError::protocol("audio channel map"))?;
            let mut channels = vec![];
            for channel in channel_map.split(',') {
                let value = device["volume"][channel]["value"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| SystemError::protocol("audio volume"))?;
                channels.push(AudioChannel {
                    name: channel.into(),
                    volume: value,
                });
            }
            let max = channels.iter().map(|c| c.volume).max().unwrap_or(0);
            result.push(AudioDevice {
                id: device["index"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| SystemError::protocol("audio id"))?,
                name: name.into(),
                description: device["description"].as_str().unwrap_or(name).into(),
                direction,
                percent: max as f64 / 65536. * 100.,
                muted: device["mute"]
                    .as_bool()
                    .ok_or_else(|| SystemError::protocol("audio mute"))?,
                is_default: name == default,
                channels,
            });
        }
        Ok(result)
    }
    fn device(&self, direction: AudioDirection, id: u32) -> Result<AudioDevice> {
        self.devices(direction, "")?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| {
                SystemError::new(
                    ErrorKind::NotFound,
                    "audio device",
                    "Device is no longer available",
                )
            })
    }
    pub fn set_volume(&self, direction: AudioDirection, id: u32, percent: f64) -> Result<()> {
        if !percent.is_finite() || !(0. ..=100.).contains(&percent) {
            return Err(SystemError::invalid(
                "audio volume",
                "Volume must be between 0 and 100 percent",
            ));
        }
        let device = self.device(direction, id)?;
        let max = device.channels.iter().map(|c| c.volume).max().unwrap_or(0);
        let mut args = vec![
            verb(direction, "set-sink-volume", "set-source-volume").to_owned(),
            id.to_string(),
        ];
        // Preserve stereo/surround balance instead of flattening every channel.
        args.extend(device.channels.iter().map(|c| {
            format!(
                "{:.6}%",
                if max == 0 {
                    percent
                } else {
                    percent * c.volume as f64 / max as f64
                }
            )
        }));
        self.0.run(&args).map(|_| ())
    }
    pub fn set_mute(&self, direction: AudioDirection, id: u32, muted: bool) -> Result<()> {
        self.device(direction, id)?;
        self.0
            .run(&[
                verb(direction, "set-sink-mute", "set-source-mute").into(),
                id.to_string(),
                if muted { "1".into() } else { "0".into() },
            ])
            .map(|_| ())
    }
    pub fn set_default(&self, direction: AudioDirection, id: u32) -> Result<()> {
        self.device(direction, id)?;
        self.0
            .run(&[
                verb(direction, "set-default-sink", "set-default-source").into(),
                id.to_string(),
            ])
            .map(|_| ())
    }
}
fn verb(direction: AudioDirection, output: &'static str, input: &'static str) -> &'static str {
    match direction {
        AudioDirection::Output => output,
        AudioDirection::Input => input,
    }
}
