//! Parallel Linux directory enumeration. Symlinks are indexed without following them.
use super::*;
use std::{
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

fn excluded(path: &Path) -> bool {
    path == Path::new("/proc")
        || path == Path::new("/sys")
        || path == Path::new("/dev")
        || path == Path::new("/run")
        || blocked(path.as_os_str().as_bytes())
}

pub fn list_one(path: &[u8]) -> Option<Listing> {
    let path = Path::new(std::ffi::OsStr::from_bytes(path));
    if excluded(path) {
        return None;
    }
    let mut listing = Listing { id: 0, names: Vec::new(), ents: Vec::new() };
    for entry in std::fs::read_dir(path).ok()?.flatten() {
        let name = entry.file_name();
        let bytes = name.as_bytes();
        let Ok(name_len) = u16::try_from(bytes.len()) else { continue };
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
        let mut kind = if meta.is_symlink() {
            KIND_LINK
        } else if meta.is_dir() {
            KIND_DIR
        } else if meta.is_file() {
            KIND_FILE
        } else {
            KIND_OTHER
        };
        if meta.is_dir() && excluded(&entry.path()) {
            kind |= FLAG_MOUNT;
        }
        let name_off = listing.names.len() as u32;
        listing.names.extend_from_slice(bytes);
        listing.ents.push(RawEnt { name_off, name_len, kind, size: meta.len(), mtime: meta.mtime().clamp(0, u32::MAX as i64) as u32, child: NONE });
    }
    LISTED.fetch_add(listing.ents.len(), Ordering::Relaxed);
    Some(listing)
}

pub fn scan(root: &[u8], threads: usize) -> Vec<Listing> {
    let next = AtomicU32::new(1);
    let out = Mutex::new(Vec::new());
    let seen = Mutex::new(std::collections::HashSet::new());
    let root = crate::sys::os_path(root);
    if let Ok(meta) = std::fs::metadata(&root) {
        seen.lock().unwrap().insert((meta.dev(), meta.ino()));
    }
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).build().unwrap();
    fn visit<'s>(
        path: std::path::PathBuf,
        id: u32,
        next: &'s AtomicU32,
        seen: &'s Mutex<std::collections::HashSet<(u64, u64)>>,
        out: &'s Mutex<Vec<Listing>>,
        scope: &rayon::Scope<'s>,
    ) {
        let mut listing = list_one(path.as_os_str().as_bytes()).unwrap_or(Listing { id, names: Vec::new(), ents: Vec::new() });
        listing.id = id;
        for entry in &mut listing.ents {
            if entry.kind & 3 != KIND_DIR {
                continue;
            }
            let name = &listing.names[entry.name_off as usize..entry.name_off as usize + entry.name_len as usize];
            let child = path.join(std::ffi::OsStr::from_bytes(name));
            if excluded(&child) {
                entry.kind |= FLAG_MOUNT;
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&child) else { continue };
            if !meta.is_dir() || !seen.lock().unwrap().insert((meta.dev(), meta.ino())) {
                entry.kind |= FLAG_MOUNT;
                continue;
            }
            let child_id = next.fetch_add(1, Ordering::Relaxed);
            entry.child = child_id;
            scope.spawn(move |scope| visit(child, child_id, next, seen, out, scope));
        }
        out.lock().unwrap().push(listing);
    }
    pool.scope(|scope| visit(root, 0, &next, &seen, &out, scope));
    out.into_inner().unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn scan_keeps_links_without_recursing() {
        let dir = std::env::temp_dir().join(format!("finder-linux-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/file"), b"data").unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let listings = super::scan(dir.as_os_str().as_bytes(), 2);
        assert_eq!(listings.len(), 2);
        assert!(listings.iter().flat_map(|l| &l.ents).any(|e| e.kind == super::KIND_LINK && e.child == super::NONE));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
