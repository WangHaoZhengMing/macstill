//! Linux Finder desktop application.
#[cfg(not(target_os = "linux"))]
compile_error!("Finder supports Linux only");
mod actions;
mod app;
mod browser;
mod model;
mod services;
mod ui;
fn main() {
    app::run();
}
