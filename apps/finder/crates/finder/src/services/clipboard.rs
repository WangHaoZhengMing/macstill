//! GNOME/Nautilus copied-files clipboard format, supported by Wayland and X11.
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
const MIME: &str = "x-special/gnome-copied-files";
fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}
fn writer() -> Command {
    if wayland() {
        let mut c = Command::new("wl-copy");
        c.args(["--type", MIME]);
        c
    } else {
        let mut c = Command::new("xclip");
        c.args(["-selection", "clipboard", "-t", MIME, "-i"]);
        c
    }
}
pub fn put_files(paths: &[PathBuf], cut: bool) -> Result<(), String> {
    let mut text = if cut { "cut" } else { "copy" }.to_owned();
    for path in paths {
        let url = url::Url::from_file_path(path).map_err(|_| "Clipboard paths must be absolute")?;
        text.push('\n');
        text.push_str(url.as_str());
    }
    let mut child = writer()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "Install {} for file clipboard support: {e}",
                if wayland() { "wl-clipboard" } else { "xclip" }
            )
        })?;
    child
        .stdin
        .take()
        .ok_or("Could not open clipboard input")?
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Could not set file clipboard".into())
    }
}
pub fn get_files() -> Option<(Vec<PathBuf>, bool)> {
    let mut c = if wayland() {
        let mut c = Command::new("wl-paste");
        c.args(["--no-newline", "--type", MIME]);
        c
    } else {
        let mut c = Command::new("xclip");
        c.args(["-selection", "clipboard", "-t", MIME, "-o"]);
        c
    };
    let output = c.stderr(Stdio::null()).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse(&String::from_utf8(output.stdout).ok()?)
}
fn parse(text: &str) -> Option<(Vec<PathBuf>, bool)> {
    let mut lines = text.lines();
    let cut = match lines.next()? {
        "cut" => true,
        "copy" => false,
        _ => return None,
    };
    let paths: Vec<_> = lines
        .filter_map(|s| url::Url::parse(s).ok()?.to_file_path().ok())
        .collect();
    (!paths.is_empty()).then_some((paths, cut))
}
pub fn clear() {
    if wayland() {
        let _ = Command::new("wl-copy").arg("--clear").status();
    } else if let Ok(mut child) = writer().stdin(Stdio::piped()).spawn() {
        drop(child.stdin.take());
        let _ = child.wait();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn file_uris_preserve_special_names() {
        let paths = vec![std::path::PathBuf::from("/tmp/a #中.txt")];
        let text = format!("cut\n{}", url::Url::from_file_path(&paths[0]).unwrap());
        assert_eq!(super::parse(&text), Some((paths, true)));
        assert!(super::parse("not files").is_none());
    }
}
