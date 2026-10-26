# Macstill

[English](README.md) · [简体中文](README.zh-CN.md) · [Deutsch](README.de.md) · [日本語](README.ja.md)

Macstill ist eine von macOS inspirierte Linux-Desktopumgebung auf Basis von **Rust, GPUI und Hyprland**. Sie vereint eine Desktop-Shell, den Dateimanager Finder, Systemeinstellungen, Systeminformationen, Bildschirmaufnahmen und AirDrop-Integration.

## Vorschau

### Desktopübersicht
![Macstill-Desktopübersicht](assets/img/overview.png)

### Demo
![Macstill-Desktopdemo](https://github.com/WangHaoZhengMing/assets/raw/main/ezgif-screen-recording-2026-10-26.mp4)

## Funktionen

- **Launchpad** — Oberfläche zum Starten und Verwalten von Anwendungen.
- **Finder** — Dateimanager mit integrierter Dateisuche.
- **Stage Manager** — Fenster gruppieren und wechseln.
- **Mission Control** — Übersicht über Arbeitsflächen und Fenster.
- **Dock** — angeheftete Anwendungen und Aktivitätsanzeigen.
- **Eigene Anwendungssymbole** — Symbole ersetzen und ungenutzte vorgegebene Symbole ausblenden.
- **Systeminformationen** — Hardware- und Systemstatus; teilweise implementiert.
- **Einstellungen** — Oberfläche, Symbole und Interaktionen werden weiterentwickelt.
- **Bildschirmaufnahmen** — teilweise implementiert, mit bekannten Fehlern.
- **Musik** — inspiriert von Windows Media Center.
- **Passwörter** — derzeit nur die Oberfläche.
- **AirDrop** — Integration auf Basis von [owly](https://github.com/t4t5/owly).

Macstill wird aktiv entwickelt. Einige Komponenten sind noch unvollständig.

## Repository-Struktur

| Verzeichnis | Inhalt |
| --- | --- |
| `apps/` | Desktopanwendungen und Einstiegspunkte |
| `crates/` | Gemeinsame Komponenten und Systemdienste |
| `plugins/` | Globales Menü und Stage-Manager-Integration für Hyprland |
| `assets/` | Symbole, Bilder und weitere Ressourcen |
| `config/` | Standardkonfiguration für Desktop und Hyprland |
| `packaging/` | Desktop-Einträge und Dienstvorlagen |
| `vendor/` | Angepasster Quellcode von Drittanbietern |

## Entwicklung

Zielplattform ist Linux mit Hyprland. Das Projekt verwendet Rust Edition 2024. Zum Kompilieren werden eine kompatible Rust-Toolchain und die nativen Bibliotheken für GPUI und die Systemintegrationen benötigt.

```sh
cargo build --workspace --locked
cargo run -p finder
cargo run -p macstill-info -- --about
```

Weitere Details enthalten die [Finder-Dokumentation](apps/finder/README.md) und die [Dokumentation der Systeminformationen](apps/info/README.md). Hyprland-Konfigurationsbeispiele liegen unter `config/`, Paketvorlagen unter `packaging/`.

## Mitwirken

Issues und Pull Requests sind willkommen. Bitte bei Fehlerberichten Schritte zur Reproduktion und Angaben zur Umgebung beifügen.

## Danksagung und Lizenz

Macstill baut auf der Arbeit der Open-Source-Community auf, insbesondere GPUI, Hyprland und [owly](https://github.com/t4t5/owly).

Der Macstill-Quellcode steht unter der [MIT-Lizenz](LICENSE). Für Code und Grafiken Dritter gelten die jeweiligen Lizenzen; Referenzgrafiken werden durch die Quellcodelizenz nicht neu lizenziert.
