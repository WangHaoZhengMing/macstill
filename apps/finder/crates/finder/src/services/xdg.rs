use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join("finder")
}

pub fn open(path: &Path) -> std::io::Result<std::process::Child> {
    std::process::Command::new("xdg-open").arg(path).spawn()
}

pub fn reveal(path: &Path) -> std::io::Result<std::process::Child> {
    open(path.parent().unwrap_or(path))
}

/// Resolve localized XDG folders (for example Documents -> Dokumente).
pub fn user_dir(name: &str) -> PathBuf {
    // Unknown XDG names return HOME on some desktops; they are ordinary subfolders.
    let key = name.to_ascii_uppercase();
    if !matches!(
        key.as_str(),
        "DESKTOP"
            | "DOWNLOAD"
            | "DOWNLOADS"
            | "DOCUMENTS"
            | "MUSIC"
            | "PICTURES"
            | "VIDEOS"
            | "TEMPLATES"
            | "PUBLICSHARE"
    ) {
        return home().join(name);
    }
    let key = if key == "DOWNLOADS" { "DOWNLOAD" } else { &key };
    if let Ok(output) = std::process::Command::new("xdg-user-dir").arg(key).output() {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout);
            let path = PathBuf::from(value.trim_end_matches('\n'));
            if path.is_absolute() {
                return path;
            }
        }
    }
    home().join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_directories_are_not_resolved_as_xdg_home() {
        assert_eq!(user_dir("Code"), home().join("Code"));
        assert_eq!(user_dir("OneDrive"), home().join("OneDrive"));
    }
    #[test]
    fn downloads_uses_the_standard_xdg_download_key() {
        assert_eq!(user_dir("Downloads"), user_dir("Download"));
    }
}
