use macstill::{
    catalog, config,
    ui::{self, Mode},
};
fn main() -> anyhow::Result<()> {
    macstill::diagnostics::event("main.entry");
    let command = std::env::args().nth(1).unwrap_or_else(|| "preview".into());
    if {
        let _span = macstill::diagnostics::span("control.request");
        macstill::control::request(&command)
    } {
        return Ok(());
    }
    match command.as_str() {
        "about" => catalog::spawn_companion("macstill-info", &["--about"])?,
        "info" => catalog::spawn_companion("macstill-info", &["--report"])?,
        "finder" => catalog::open_finder(&std::env::args_os().skip(2).collect::<Vec<_>>())?,
        "mission-control" => {
            let action = std::env::args().nth(2).unwrap_or_else(|| "toggle".into());
            match action.as_str() {
                "next" => macstill::hyprland::mission::adjacent(1)?,
                "previous" => macstill::hyprland::mission::adjacent(-1)?,
                "toggle" | "open" | "close" | "gesture" => anyhow::ensure!(
                    macstill::control::request(&format!("mission-{action}")),
                    "Start macstill shell inside Hyprland first"
                ),
                _ => anyhow::bail!(
                    "Usage: macstill mission-control [toggle|open|close|next|previous]"
                ),
            }
        }
        "stage" => {
            let args: Vec<_> = std::env::args().skip(2).collect();
            anyhow::ensure!(
                args.len() == 1
                    && matches!(
                        args[0].as_str(),
                        "toggle"
                            | "on"
                            | "off"
                            | "next"
                            | "ungroup"
                            | "desktop"
                            | "hide-recent"
                            | "show-recent"
                    ),
                "Usage: macstill stage toggle|on|off|next|ungroup|desktop|hide-recent|show-recent"
            );
            anyhow::ensure!(
                macstill::control::request(&format!("stage-{}", args[0])),
                "Start macstill shell inside Hyprland before controlling Stage Manager"
            );
        }
        "screenshot" => catalog::spawn_companion(
            "macstill-screenshot",
            &std::env::args()
                .skip(2)
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )?,
        "airdrop" => catalog::spawn_companion("macstill-airdrop", &[])?,
        "media" => {
            use macstill_system::media::Command;
            let command = match std::env::args().nth(2).as_deref() {
                Some("play-pause") => Command::PlayPause,
                Some("play") => Command::Play,
                Some("pause") => Command::Pause,
                Some("stop") => Command::Stop,
                Some("next") => Command::Next,
                Some("previous") => Command::Previous,
                _ => {
                    anyhow::bail!("Usage: macstill media play-pause|play|pause|stop|next|previous")
                }
            };
            let api = macstill_system::SystemApi::new();
            if let Some(player) = api.media().current(None)? {
                api.media().command(&player, command)?;
            }
        }
        "settings" => catalog::spawn_companion(
            "macstill-settings",
            &std::env::args()
                .skip(2)
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )?,
        "preview" | "--preview" => ui::run(Mode::Preview),
        "launchpad" => ui::run(Mode::Launchpad),
        "desktop" => ui::run(Mode::Desktop),
        "shell" => {
            anyhow::ensure!(
                std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
                "Start this inside Hyprland. To try it in another desktop: macstill preview"
            );
            ui::run(Mode::Bar);
        }
        "window" => {
            let args: Vec<_> = std::env::args().skip(2).collect();
            anyhow::ensure!(
                args.len() == 1,
                "Usage: macstill window next|previous|same-app|center|left|right|maximize|fullscreen|close|float"
            );
            macstill::hyprland::manage_window(args[0].parse()?)?;
        }
        "apps" => println!("{}", serde_json::to_string_pretty(&catalog::scan())?),
        "config" => println!("{}", serde_json::to_string_pretty(&config::load()?)?),
        "help" | "--help" | "-h" => println!(
            "macstill [about|info|finder|mission-control|stage|screenshot|settings|preview|shell|desktop|launchpad|window|media|apps|config]\n\nabout: About Macstill summary window\ninfo: System Information report\nfinder: Built-in file manager [PATH|--reveal PATH|--trash|--airdrop], in its own process\nstage: Stage Manager toggle|on|off|next|ungroup|desktop|hide-recent|show-recent (running shell required)\nshell: Hyprland desktop + top bar + Dock\nsettings: System Settings (also works in GNOME)\npreview: a regular window; no session changes\nwindow: desktop window switching, placement and zoom\napps: installed desktop application catalog as JSON"
        ),
        _ => anyhow::bail!("Unknown command {command:?}; run macstill --help"),
    }
    Ok(())
}
