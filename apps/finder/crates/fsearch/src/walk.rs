//! Parallel Linux enumeration and compact directory listing types.

use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

pub const NONE: u32 = u32::MAX;

/// Optional subtrees excluded by the caller.
pub static SKIP: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();

/// Paths that could not be read during enumeration.
pub static DENIED: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Entries listed by scans so far: the first scan's progress.
pub static LISTED: AtomicUsize = AtomicUsize::new(0);

pub fn blocked(path: &[u8]) -> bool {
    SKIP.get().is_some_and(|v| v.iter().any(|s| path.starts_with(s) && (path.len() == s.len() || path[s.len()] == b'/')))
}

pub const KIND_FILE: u8 = 0;
pub const KIND_DIR: u8 = 1;
pub const KIND_LINK: u8 = 2;
pub const KIND_OTHER: u8 = 3;

/// Hidden-entry flag in the on-disk index format.
pub const FLAG_HIDDEN: u8 = 1 << 2;
/// Directory that is a mount point we did not descend into.
pub const FLAG_MOUNT: u8 = 1 << 3;

#[derive(Clone, Copy)]
pub struct RawEnt {
    pub name_off: u32,
    pub name_len: u16,
    /// kind in the low 2 bits, FLAG_* above.
    pub kind: u8,
    pub size: u64,
    pub mtime: u32,
    /// Temp id of the listing for this directory, or NONE.
    pub child: u32,
}

pub struct Listing {
    pub id: u32,
    pub names: Vec<u8>,
    pub ents: Vec<RawEnt>,
}

mod linux;

pub use linux::{list_one, scan};
