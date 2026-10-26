mod collect;
mod control;
mod model;
mod request;
mod ui;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!(
            "macstill-info [--about|--report|--json]\n\nAbout opens the compact computer summary. More Info opens a separate System Information window.\n--json prints the same read-only inventory without starting the GUI."
        );
        return Ok(());
    }
    if args == ["--json"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&collect::collect(std::path::Path::new("/")))?
        );
        return Ok(());
    }
    let request = request::Request::parse(&args)?;
    if control::request(&request) {
        return Ok(());
    }
    let server = control::Server::bind()?;
    ui::run(request, server);
    Ok(())
}
