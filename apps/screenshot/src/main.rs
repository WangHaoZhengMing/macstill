mod capture;
mod control;
mod model;
mod ui;
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!(
            "macstill-screenshot [--fullscreen|--region|--window] [--clipboard] [--output DIRECTORY]\nWithout arguments, opens the Screenshot toolbar. Escape cancels; Space switches area/window selection.\nRequires Hyprland. Copy uses wl-clipboard."
        );
        return Ok(());
    }
    let request = model::Request::parse(&args)?;
    if control::request(&request) {
        return Ok(());
    }
    let server = control::Server::bind()?;
    ui::run(request, server);
    Ok(())
}
