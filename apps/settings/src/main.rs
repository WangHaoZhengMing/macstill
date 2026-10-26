mod backend;
mod control;
mod request;
mod system_backend;
mod system_cli;
mod ui;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--run-login-items"] {
        macstill_system::login_items::launch()?;
        return Ok(());
    }
    if system_cli::run(&args)? {
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|a| matches!(a.as_str(), "--help" | "-h"))
    {
        println!(
            "macstill-settings [--page general|airdrop|storage|login-items|wifi|bluetooth|network|sound|displays|battery|date-time|language-region|keyboard|mouse|trackpad|lock-screen|appearance|control-center|wallpaper|dock|shortcuts|applications|about | --bind SHORTCUT_ID | --icon APP_ID | --system-info | --system-query FILE | --system-apply FILE]"
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
