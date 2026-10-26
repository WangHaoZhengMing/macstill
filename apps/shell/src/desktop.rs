//! Files backing the desktop surface. Imports copy originals and never overwrite.
use anyhow::{Context, Result, bail};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
}
pub fn directory() -> PathBuf {
    gio::glib::user_special_dir(gio::glib::UserDirectory::Desktop)
        .filter(|p| *p != gio::glib::home_dir())
        .unwrap_or_else(|| gio::glib::home_dir().join("Desktop"))
}
pub fn entries(root: &Path) -> Result<Vec<Entry>> {
    fs::create_dir_all(root)?;
    let mut entries = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| Entry {
            directory: e.path().is_dir(),
            path: e.path(),
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    Ok(entries)
}
fn candidate(root: &Path, name: &std::ffi::OsStr, index: usize, directory: bool) -> PathBuf {
    if index == 0 {
        return root.join(name);
    }
    let name = Path::new(name);
    let mut stem = if directory {
        name.as_os_str()
    } else {
        name.file_stem().unwrap_or(name.as_os_str())
    }
    .to_os_string();
    stem.push(if index == 1 {
        " copy".to_string()
    } else {
        format!(" copy {index}")
    });
    if !directory {
        if let Some(ext) = name.extension() {
            stem.push(".");
            stem.push(ext);
        }
    }
    root.join(stem)
}
pub fn new_folder(root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root)?;
    for i in 0..10000 {
        let name = if i == 0 {
            "untitled folder".into()
        } else {
            format!("untitled folder {}", i + 1)
        };
        let path = root.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    bail!("Could not find an unused folder name")
}
fn copy_item(source: &Path, target: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        return std::os::unix::fs::symlink(fs::read_link(source)?, target);
    }
    if meta.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_item(&entry.path(), &target.join(entry.file_name()))?;
        }
        Ok(())
    } else if meta.is_file() {
        let mut input = fs::File::open(source)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        io::copy(&mut input, &mut output)?;
        fs::set_permissions(target, meta.permissions())
    } else {
        Err(io::Error::other("Unsupported file type"))
    }
}
pub fn import(root: &Path, paths: &[PathBuf]) -> Result<()> {
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    for source in paths {
        let name = source
            .file_name()
            .context("Cannot copy a filesystem root onto the desktop")?;
        let parent = source
            .parent()
            .context("Missing source directory")?
            .canonicalize()?;
        if parent == root {
            continue;
        }
        let meta = fs::symlink_metadata(source)?;
        if meta.is_dir() && root.starts_with(source.canonicalize()?) {
            bail!("Cannot copy a folder into itself");
        }
        let mut copied = false;
        for index in 0..10000 {
            let target = candidate(&root, name, index, meta.is_dir());
            // Check dangling symlinks too. create_new/create_dir also protect races.
            if fs::symlink_metadata(&target).is_ok() {
                continue;
            }
            copy_item(source, &target)
                .with_context(|| format!("Could not copy {}", source.display()))?;
            copied = true;
            break;
        }
        if !copied {
            bail!("Could not find an unused filename");
        }
    }
    Ok(())
}

/// Move a desktop entry without replacing an existing file (including dangling links).
pub fn rename(path: &Path, name: &str) -> Result<PathBuf> {
    use gio::prelude::*;
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        bail!("Enter a filename without slashes");
    }
    let target = path
        .parent()
        .context("Missing parent directory")?
        .join(name);
    if target == path {
        return Ok(target);
    }
    gio::File::for_path(path)
        .move_(
            &gio::File::for_path(&target),
            gio::FileCopyFlags::NONE,
            gio::Cancellable::NONE,
            None,
        )
        .with_context(|| format!("Could not rename {}", path.display()))?;
    Ok(target)
}
pub fn duplicate(path: &Path) -> Result<PathBuf> {
    let root = path.parent().context("Missing parent directory")?;
    let name = path.file_name().context("Missing filename")?;
    let directory = fs::symlink_metadata(path)?.is_dir();
    for index in 1..10000 {
        let target = candidate(root, name, index, directory);
        if fs::symlink_metadata(&target).is_ok() {
            continue;
        }
        copy_item(path, &target)?;
        return Ok(target);
    }
    bail!("Could not find an unused filename")
}
pub fn trash(path: &Path) -> Result<()> {
    use gio::prelude::*;
    gio::File::for_path(path)
        .trash(gio::Cancellable::NONE)
        .with_context(|| format!("Could not move {} to Trash", path.display()))
}

pub type Positions = std::collections::BTreeMap<PathBuf, [f32; 2]>;
pub fn load_positions(path: &Path) -> Positions {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Positions>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, p)| p.iter().all(|v| v.is_finite()))
        .collect()
}
pub fn save_positions(path: &Path, positions: &Positions) -> Result<()> {
    let parent = path.parent().context("Missing layout directory")?;
    fs::create_dir_all(parent)?;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, serde_json::to_vec(positions)?)?;
    fs::rename(temp, path)?;
    Ok(())
}
pub fn clamp_position(position: [f32; 2], width: f32, height: f32) -> [f32; 2] {
    [
        position[0].clamp(0., (width - 92.).max(0.)),
        position[1].clamp(34., (height - 180.).max(34.)),
    ]
}
