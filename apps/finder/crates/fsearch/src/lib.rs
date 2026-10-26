//! Linux file search: fuzzy filename index, live filesystem watching and text content search.
#[cfg(not(target_os = "linux"))]
compile_error!("fsearch supports Linux only");

pub mod content;
mod engine;
pub mod sys;

pub mod index;
pub mod live;
pub mod query;
pub mod walk;

pub use content::{FileMatches, Grep, GrepResult};
pub use engine::{Engine, Found, Options, Status, default_dir};
pub use query::{GrepMode, Query};

#[path = "watch_linux.rs"]
mod watch;

#[cfg(test)]
mod linux_tests;
