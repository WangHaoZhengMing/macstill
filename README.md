# Macstill

[English](README.md) · [简体中文](README.zh-CN.md) · [Deutsch](README.de.md) · [日本語](README.ja.md)

A macOS-inspired Linux desktop environment built with **Rust, GPUI, and Hyprland**. Macstill brings together a desktop shell, Finder file manager, system settings, system information, screenshot tools, and AirDrop integration.

## Preview

### Desktop overview
![Macstill desktop overview](assets/img/overview.png)

### In action
![Macstill desktop demo](https://pic1.imgdb.cn/i/034mf8cvlHveAjkNNQ4RXV.gif)

## Features

- **Launchpad** — application launcher and management interface.
- **Finder** — file manager with integrated file search.Edited From [Pranav Karthik](https://github.com/pranavkarthik10/Finder?tab=readme-ov-file)

- **Stage Manager** — window grouping and switching.
- **Mission Control** — workspace and window overview.
- **Dock** — pinned applications and running indicators.
- **Custom application icons** — replace icons and hide unused preset icons.
- **System information** — hardware and system status panels; partially implemented.
- **Settings** — interface, icons, and interactions are under active development.
- **Screenshots** — partially implemented, with known bugs.
- **Music** — inspired by Windows Media Center.
- **Passwords** — UI only.
- **AirDrop** — integration based on [owly](https://github.com/t4t5/owly).

Macstill is under active development. Some components are incomplete.

## Repository layout

| Directory | Contents |
| --- | --- |
| `apps/` | Desktop applications and entry points |
| `crates/` | Shared components and system services |
| `plugins/` | Hyprland global menu and Stage Manager integration |
| `assets/` | Icons, images, and other static resources |
| `config/` | Default desktop and Hyprland configuration |
| `packaging/` | Desktop entries and service templates |
| `vendor/` | Patched third-party source code |

## Development

The project targets Linux with Hyprland and uses Rust edition 2024. Building requires a compatible Rust toolchain and the native libraries required by GPUI and the system integrations.

```sh
cargo build --workspace --locked
cargo run -p finder
cargo run -p macstill-info -- --about
```

See the [Finder documentation](apps/finder/README.md) and [system information documentation](apps/info/README.md) for component details. Hyprland configuration examples are in `config/`; packaging templates are in `packaging/`.

## Contributing

Issues and pull requests are welcome. Include reproduction steps and relevant environment details when reporting a bug.

## Acknowledgments and license

Macstill builds on the work of the open-source community, including GPUI, Hyprland, and [owly](https://github.com/t4t5/owly).

Macstill source code is licensed under the [MIT License](LICENSE). Third-party code and artwork retain their respective licenses; the source license does not relicense reference artwork.
