//! Cancellable file operations. Copies are staged beside the destination before commit.
use super::jobs::{self, Change, Conflict, Decision};
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub enum Op {
    Copy,
    Move,
    Trash,
}

fn rename_noreplace(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let src = std::ffi::CString::new(src.as_os_str().as_bytes())?;
    let dst = std::ffi::CString::new(dst.as_os_str().as_bytes())?;
    if unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            src.as_ptr(),
            libc::AT_FDCWD,
            dst.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
fn exists(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
fn metadata(src: &Path, dst: &Path, m: &std::fs::Metadata) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dst.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // Ownership changes precede mode/ACL restoration (chown may clear set-id bits).
    if unsafe { libc::lchown(c.as_ptr(), m.uid(), m.gid()) } != 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EPERM) {
            return Err(e.to_string());
        }
        let actual = std::fs::symlink_metadata(dst).map_err(|e| e.to_string())?;
        if actual.uid() != m.uid() || actual.gid() != m.gid() {
            jobs::warning(format!(
                "{}: ownership could not be preserved",
                src.display()
            ));
        }
    }
    if !m.is_symlink() {
        std::fs::set_permissions(dst, m.permissions()).map_err(|e| e.to_string())?;
    }
    match xattr::list(src) {
        Ok(names) => {
            for name in names {
                if let Some(value) = xattr::get(src, &name).map_err(|e| e.to_string())? {
                    xattr::set(dst, &name, &value).map_err(|e| {
                        format!(
                            "Cannot preserve attribute {:?} on {}: {e}",
                            name,
                            dst.display()
                        )
                    })?;
                }
            }
        }
        Err(e) if e.raw_os_error() == Some(libc::ENOTSUP) => {}
        Err(e) => return Err(e.to_string()),
    }
    filetime::set_symlink_file_times(
        dst,
        filetime::FileTime::from_last_access_time(m),
        filetime::FileTime::from_last_modification_time(m),
    )
    .map_err(|e| e.to_string())
}
fn copy_tree(
    src: &Path,
    dst: &Path,
    links: &mut HashMap<(u64, u64), PathBuf>,
) -> Result<(), String> {
    jobs::checkpoint()?;
    jobs::advance(src, 0);
    let m = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if m.is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(src).map_err(|e| e.to_string())?, dst)
            .map_err(|e| e.to_string())?;
    } else if m.is_dir() {
        std::fs::create_dir(dst).map_err(|e| e.to_string())?;
        for item in std::fs::read_dir(src).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            copy_tree(&item.path(), &dst.join(item.file_name()), links)?;
        }
    } else if m.is_file() {
        let key = (m.dev(), m.ino());
        if let Some(first) = links.get(&key) {
            std::fs::hard_link(first, dst).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let mut input = std::fs::File::open(src).map_err(|e| e.to_string())?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dst)
            .map_err(|e| e.to_string())?;
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            jobs::checkpoint()?;
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            jobs::advance(src, n as u64);
        }
        output.sync_all().map_err(|e| e.to_string())?;
        if m.nlink() > 1 {
            links.insert(key, dst.into());
        }
    } else {
        return Err(format!(
            "Copying special file {} is unsupported",
            src.display()
        ));
    }
    metadata(src, dst, &m)
}
fn check_destination(src: &Path, dst: &Path) -> Result<(), String> {
    let m = std::fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if m.is_dir() {
        let source = std::fs::canonicalize(src).map_err(|e| e.to_string())?;
        let parent = std::fs::canonicalize(dst.parent().ok_or("Missing parent folder")?)
            .map_err(|e| e.to_string())?;
        if parent.starts_with(source) {
            return Err("Cannot copy or move a folder into itself".into());
        }
    }
    Ok(())
}
fn copy(src: &Path, dst: &Path) -> Result<(), String> {
    check_destination(src, dst)?;
    let temp = tempfile::Builder::new()
        .prefix(".finder-copy-")
        .tempdir_in(dst.parent().ok_or("Missing parent")?)
        .map_err(|e| e.to_string())?;
    let stage = temp.path().join("item");
    copy_tree(src, &stage, &mut HashMap::new())?;
    jobs::checkpoint()?;
    rename_noreplace(&stage, dst).map_err(|e| e.to_string())
}
fn move_file(src: &Path, dst: &Path) -> Result<(), String> {
    check_destination(src, dst)?;
    match rename_noreplace(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy(src, dst)?;
            // The destination is complete. A failed source removal must remain recoverable.
            jobs::record(Change::RecoveryCopy {
                source: src.into(),
                destination: dst.into(),
            });
            if std::fs::symlink_metadata(src)
                .map_err(|e| e.to_string())?
                .is_dir()
            {
                std::fs::remove_dir_all(src)
            } else {
                std::fs::remove_file(src)
            }
            .map_err(|e| {
                format!(
                    "Copied to {}, but could not fully remove {}: {e}",
                    dst.display(),
                    src.display()
                )
            })?;
            if let Some(j) = jobs::current() {
                j.changes.lock().unwrap().pop();
            }
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}
pub fn trash_one(path: &Path) -> Result<(), String> {
    jobs::checkpoint()?;
    let before = trash::os_limited::list().map_err(|e| e.to_string())?;
    trash::delete(path).map_err(|e| e.to_string())?;
    let item = trash::os_limited::list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|item| item.original_path() == path && !before.iter().any(|old| old.id == item.id))
        .ok_or("File was trashed but its restore record could not be found")?;
    jobs::record(Change::Trashed {
        id: PathBuf::from(item.id),
        name: PathBuf::from(item.name),
        parent: item.original_parent,
        deleted: item.time_deleted,
    });
    Ok(())
}
fn transfer(src: &Path, dst: &Path, moving: bool) -> Result<(), String> {
    jobs::checkpoint()?;
    check_destination(src, dst)?;
    if src == dst {
        return Err("Source and destination are the same".into());
    }
    if let (Ok(a), Ok(b)) = (
        std::fs::symlink_metadata(src),
        std::fs::symlink_metadata(dst),
    ) {
        if a.dev() == b.dev() && a.ino() == b.ino() {
            return Err("Source and destination refer to the same item".into());
        }
    }
    let mut dst = dst.to_path_buf();
    let mut replace = false;
    if exists(&dst)? {
        let directories = std::fs::symlink_metadata(src)
            .map_err(|e| e.to_string())?
            .is_dir()
            && std::fs::symlink_metadata(&dst)
                .map_err(|e| e.to_string())?
                .is_dir();
        match jobs::conflict(Conflict {
            source: src.into(),
            destination: dst.clone(),
            directories,
        })? {
            Decision::Skip => return Ok(()),
            Decision::Cancel => return Err("Operation cancelled".into()),
            Decision::KeepBoth => dst = copy_name(&dst),
            Decision::Replace => replace = true,
            Decision::Merge if directories => {
                for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    transfer(&entry.path(), &dst.join(entry.file_name()), moving)?;
                }
                // Retain the source directory so skipped items and directory metadata survive.
                return Ok(());
            }
            Decision::Merge => return Err("Only folders can be merged".into()),
        }
    }
    // Replacement is staged before touching the old target. A backup is restored on failure.
    let backup = if replace {
        let temp = tempfile::Builder::new()
            .prefix(".finder-backup-")
            .tempdir_in(dst.parent().unwrap())
            .map_err(|e| e.to_string())?;
        Some(temp)
    } else {
        None
    };
    let prepared = if replace {
        let temp = tempfile::Builder::new()
            .prefix(".finder-copy-")
            .tempdir_in(dst.parent().unwrap())
            .map_err(|e| e.to_string())?;
        copy_tree(src, &temp.path().join("item"), &mut HashMap::new())?;
        jobs::checkpoint()?;
        Some(temp)
    } else {
        None
    };
    if let Some(backup) = &backup {
        rename_noreplace(&dst, &backup.path().join("previous")).map_err(|e| e.to_string())?;
    }
    let result = if let Some(stage) = &prepared {
        rename_noreplace(&stage.path().join("item"), &dst).map_err(|e| e.to_string())
    } else if moving {
        move_file(src, &dst)
    } else {
        copy(src, &dst)
    };
    if let Err(error) = result {
        if let Some(backup) = backup {
            let saved = backup.keep().join("previous");
            rename_noreplace(&saved, &dst)
                .map_err(|e| format!("{error}; previous item saved at {}: {e}", saved.display()))?;
            let _ = std::fs::remove_dir(saved.parent().unwrap());
        }
        return Err(error);
    }
    if let Some(backup) = backup {
        let saved = backup.keep().join("previous");
        jobs::record(Change::Moved {
            from: dst.clone(),
            to: saved,
        });
    }
    if moving && prepared.is_some() {
        jobs::record(Change::RecoveryCopy {
            source: src.into(),
            destination: dst.clone(),
        });
        if std::fs::symlink_metadata(src)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            std::fs::remove_dir_all(src)
        } else {
            std::fs::remove_file(src)
        }
        .map_err(|e| e.to_string())?;
        if let Some(j) = jobs::current() {
            j.changes.lock().unwrap().pop();
        }
    }
    jobs::record(if moving {
        Change::Moved {
            from: src.into(),
            to: dst,
        }
    } else {
        Change::Created(dst)
    });
    jobs::completed();
    Ok(())
}
pub fn run(op: Op, from: &[PathBuf], into: Option<&Path>) -> Result<(), String> {
    for path in from {
        jobs::checkpoint()?;
        match op {
            Op::Trash => {
                trash_one(path)?;
                jobs::completed();
            }
            Op::Copy | Op::Move => {
                let dst = into
                    .ok_or("Missing destination folder")?
                    .join(path.file_name().ok_or("Cannot copy a filesystem root")?);
                transfer(path, &dst, matches!(op, Op::Move))?;
            }
        }
    }
    Ok(())
}
pub fn copy_as(from: &[PathBuf], to: &[PathBuf]) -> Result<(), String> {
    if from.len() != to.len() {
        return Err("Mismatched source and destination counts".into());
    }
    for (src, dst) in from.iter().zip(to) {
        transfer(src, dst, false)?;
    }
    Ok(())
}
pub fn undo_change(change: &Change) -> Result<(), String> {
    jobs::checkpoint()?;
    match change {
        Change::RecoveryCopy {
            source,
            destination,
        } => Err(format!(
            "The source {} was only partly removed. Keep the complete copy at {} and reconcile these paths before undoing this item.",
            source.display(),
            destination.display()
        )),
        Change::Created(path) => trash::delete(path).map_err(|e| e.to_string()),
        Change::Moved { from, to } => {
            move_file(to, from)?;
            if to.file_name().is_some_and(|n| n == "previous")
                && to
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|n| n.to_string_lossy().starts_with(".finder-backup-"))
            {
                let _ = std::fs::remove_dir(to.parent().unwrap());
            }
            Ok(())
        }
        Change::Trashed {
            id,
            name,
            parent,
            deleted,
        } => super::trash::restore(vec![trash::TrashItem {
            id: id.clone().into_os_string(),
            name: name.clone().into_os_string(),
            original_parent: parent.clone(),
            time_deleted: *deleted,
        }]),
    }
}
#[cfg(test)]
pub fn restore(paths: &[PathBuf]) -> Result<(), String> {
    let items = trash::os_limited::list().map_err(|e| e.to_string())?;
    for path in paths {
        let item = items
            .iter()
            .filter(|i| i.original_path() == *path)
            .max_by_key(|i| i.time_deleted)
            .ok_or_else(|| format!("{} is not in the Trash", path.display()))?;
        super::trash::restore(vec![item.clone()])?;
    }
    Ok(())
}

/// Finder's names for copies: "name copy.ext", then "name copy 2.ext"...
pub fn copy_name(p: &Path) -> PathBuf {
    let dir = p.parent().unwrap_or(Path::new(""));
    let is_dir = p.is_dir();
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !is_dir && !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    (1..)
        .map(|n| {
            dir.join(if n == 1 {
                format!("{stem} copy{ext}")
            } else {
                format!("{stem} copy {n}{ext}")
            })
        })
        .find(|c| std::fs::symlink_metadata(c).is_err())
        .unwrap()
}

/// "untitled folder", "untitled folder 2", ... in `dir`, created.
pub fn new_folder(dir: &Path) -> Result<PathBuf, String> {
    let p = (1..)
        .map(|n| {
            dir.join(if n == 1 {
                "untitled folder".to_string()
            } else {
                format!("untitled folder {n}")
            })
        })
        .find(|c| std::fs::symlink_metadata(c).is_err())
        .unwrap();
    std::fs::create_dir(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Rename in place; names with path separators or reserved characters are
/// refused to preserve existing files.
pub fn rename(from: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains('\0') || name == "." || name == ".." {
        return Err(format!("The name \u{201c}{name}\u{201d} can't be used."));
    }
    let to = from.with_file_name(name);
    if to != from && std::fs::symlink_metadata(&to).is_ok() {
        return Err(format!(
            "The name \u{201c}{name}\u{201d} is already taken. Please choose a different name."
        ));
    }
    if to != from {
        move_file(from, &to)?;
    }
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recycle_and_restore() {
        let d = scratch("restore");
        let f = d.join("restore-me.txt");
        std::fs::write(&f, "back").unwrap();
        run(Op::Trash, std::slice::from_ref(&f), None).unwrap();
        assert!(!f.exists());
        restore(std::slice::from_ref(&f)).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "back");
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join("finder-ops-tests").join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copy_names_follow_finder() {
        let d = scratch("names");
        std::fs::write(d.join("a.txt"), "x").unwrap();
        assert_eq!(copy_name(&d.join("a.txt")), d.join("a copy.txt"));
        std::fs::write(d.join("a copy.txt"), "x").unwrap();
        assert_eq!(copy_name(&d.join("a.txt")), d.join("a copy 2.txt"));
        std::fs::create_dir(d.join("my.folder")).unwrap();
        assert_eq!(copy_name(&d.join("my.folder")), d.join("my.folder copy"));
    }

    #[test]
    fn new_folders_count_up() {
        let d = scratch("new");
        assert_eq!(new_folder(&d).unwrap(), d.join("untitled folder"));
        assert_eq!(new_folder(&d).unwrap(), d.join("untitled folder 2"));
    }

    #[test]
    fn rename_checks_names() {
        let d = scratch("rename");
        std::fs::write(d.join("a.txt"), "x").unwrap();
        std::fs::write(d.join("b.txt"), "x").unwrap();
        assert!(rename(&d.join("a.txt"), "b.txt").is_err());
        assert!(rename(&d.join("a.txt"), "x/y").is_err());
        assert!(rename(&d.join("a.txt"), "  ").is_err());
        assert_eq!(rename(&d.join("a.txt"), "c.txt").unwrap(), d.join("c.txt"));
        // Changing only the case is allowed.
        assert_eq!(rename(&d.join("c.txt"), "C.txt").unwrap(), d.join("C.txt"));
    }

    #[test]
    fn shell_copy_move_and_copy_as() {
        let d = scratch("shell");
        let (src, dst) = (d.join("src"), d.join("dst"));
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("f.txt"), "hello").unwrap();
        std::fs::write(src.join("sub").join("g.txt"), "deep").unwrap();
        std::fs::create_dir(&dst).unwrap();
        run(Op::Copy, &[src.join("f.txt"), src.join("sub")], Some(&dst)).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("f.txt")).unwrap(), "hello");
        assert_eq!(
            std::fs::read_to_string(dst.join("sub").join("g.txt")).unwrap(),
            "deep"
        );
        run(Op::Move, &[dst.join("f.txt")], Some(&src.join("sub"))).unwrap();
        assert!(!dst.join("f.txt").exists() && src.join("sub").join("f.txt").exists());
        let c = copy_name(&src.join("f.txt"));
        copy_as(&[src.join("f.txt")], std::slice::from_ref(&c)).unwrap();
        assert_eq!(c, src.join("f copy.txt"));
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "hello");
    }
    #[test]
    fn copy_rejects_descendants_and_preserves_symlinks() {
        let d = scratch("safe-copy");
        let src = d.join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("file"), "original").unwrap();
        std::os::unix::fs::symlink("missing", src.join("link")).unwrap();
        assert!(copy(&src, &src.join("nested")).is_err());
        let dst = d.join("dst");
        copy(&src, &dst).unwrap();
        assert_eq!(
            std::fs::read_link(dst.join("link")).unwrap(),
            PathBuf::from("missing")
        );
        assert!(copy(&src.join("file"), &dst.join("file")).is_err());
        assert!(move_file(&src.join("file"), &dst.join("file")).is_err());
        assert!(src.join("file").exists());
        std::fs::remove_dir_all(d).unwrap();
    }
}

use crate::model::entry::{Entry, display_name};
pub fn entry_from_path(path: PathBuf) -> Option<Entry> {
    let metadata = std::fs::metadata(&path).ok()?;
    let name = display_name(&path);
    Some(entry_from_metadata(name, path, &metadata))
}
fn entry_from_metadata(name: String, path: PathBuf, metadata: &std::fs::Metadata) -> Entry {
    let mut entry = Entry::from_parts(
        name,
        path,
        metadata.is_dir(),
        metadata.len(),
        metadata.modified().ok(),
    );
    entry.created = metadata.created().ok();
    entry.accessed = metadata.accessed().ok();
    entry
}
/// The folder's visible entries (no hidden or system files, like Finder).
pub fn list_hidden(dir: &Path, show_hidden: bool) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let Ok(e) = e else { continue };
        let Ok(m) = e.metadata() else { continue };
        // FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM

        let name = e.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        out.push(entry_from_metadata(name, e.path(), &m));
    }
    Ok(out)
}
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    Some(stat.f_bavail.saturating_mul(stat.f_frsize))
}

#[cfg(test)]
mod operation_regressions {
    use super::super::jobs::Job;
    use super::*;
    use std::sync::Arc;

    #[test]
    fn failed_batch_keeps_exact_undo_for_completed_items() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let dest = dir.path().join("dest");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&dest).unwrap();
        let first = source.join("first");
        std::fs::write(&first, b"payload").unwrap();
        let job = Arc::new(Job::default());
        let result = jobs::scoped(job.clone(), || {
            run(
                Op::Move,
                &[first.clone(), source.join("missing")],
                Some(&dest),
            )
        });
        assert!(result.is_err());
        assert!(!first.exists());
        let changes = job.changes.lock().unwrap().clone();
        assert_eq!(changes.len(), 1);
        undo_change(&changes[0]).unwrap();
        assert_eq!(std::fs::read(first).unwrap(), b"payload");
    }

    #[test]
    fn failed_copy_never_publishes_partial_directory() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("regular"), b"data").unwrap();
        let fifo = std::ffi::CString::new(source.join("pipe").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let destination = dir.path().join("destination");
        assert!(copy(&source, &destination).is_err());
        assert!(!destination.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn replacement_undo_restores_previous_contents() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&destination, b"old").unwrap();
        let job = Arc::new(Job::default());
        job.answer(Decision::Replace, false);
        jobs::scoped(job.clone(), || copy_as(&[source], &[destination.clone()])).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        let changes = job.changes.lock().unwrap().clone();
        for change in changes.iter().rev() {
            undo_change(change).unwrap();
        }
        assert_eq!(std::fs::read(destination).unwrap(), b"old");
    }

    #[test]
    fn skip_has_no_undo_and_cancel_preserves_destination() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&destination, b"old").unwrap();
        let job = Arc::new(Job::default());
        job.answer(Decision::Skip, true);
        jobs::scoped(job.clone(), || {
            copy_as(&[source.clone()], &[destination.clone()])
        })
        .unwrap();
        assert!(job.changes.lock().unwrap().is_empty());
        job.cancel();
        assert!(jobs::scoped(job, || copy_as(&[source], &[destination.clone()])).is_err());
        assert_eq!(std::fs::read(destination).unwrap(), b"old");
    }

    #[test]
    fn directory_copy_preserves_links_attributes_and_modification_time() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        let a = source.join("a");
        std::fs::write(&a, b"data").unwrap();
        std::fs::hard_link(&a, source.join("b")).unwrap();
        std::os::unix::fs::symlink("missing", source.join("broken-link")).unwrap();
        let time = filetime::FileTime::from_unix_time(100000, 123);
        filetime::set_file_mtime(&a, time).unwrap();
        xattr::set(&a, "user.finder-test", b"attribute").unwrap();
        copy(&source, &destination).unwrap();
        let copied = std::fs::metadata(destination.join("a")).unwrap();
        assert_eq!(
            copied.ino(),
            std::fs::metadata(destination.join("b")).unwrap().ino()
        );
        assert_eq!(
            filetime::FileTime::from_last_modification_time(&copied),
            time
        );
        assert_eq!(
            xattr::get(destination.join("a"), "user.finder-test")
                .unwrap()
                .unwrap(),
            b"attribute"
        );
        assert_eq!(
            std::fs::read_link(destination.join("broken-link")).unwrap(),
            PathBuf::from("missing")
        );
    }

    #[test]
    fn hidden_files_are_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("visible"), b"").unwrap();
        std::fs::write(dir.path().join(".hidden"), b"").unwrap();
        assert_eq!(list_hidden(dir.path(), false).unwrap().len(), 1);
        assert_eq!(list_hidden(dir.path(), true).unwrap().len(), 2);
    }
}
