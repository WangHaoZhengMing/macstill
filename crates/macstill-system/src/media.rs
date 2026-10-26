//! MPRIS client for shell surfaces and multimedia key bindings.
use crate::{dbus::*, error::*};
use gio::glib::{self, variant::ToVariant};
pub const PATH: &str = "/org/mpris/MediaPlayer2";
pub const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    pub service: String,
    pub identity: String,
    pub status: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub art_url: Option<String>,
    pub track_id: Option<String>,
    pub length: i64,
    pub position: i64,
    pub volume: f64,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}
impl Player {
    pub fn playing(&self) -> bool {
        self.status == "Playing"
    }
    pub fn can_toggle(&self) -> bool {
        self.can_control
            && if self.playing() {
                self.can_pause
            } else {
                self.can_play
            }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Seek(i64),
    SetPosition(i64),
    Volume(f64),
}
pub struct Media<'a, T>(pub(crate) &'a T);
impl<T: Transport> Media<'_, T> {
    pub fn players(&self) -> Result<Vec<Player>> {
        let (mut names,): (Vec<String>,) = decode(
            self.0.call(
                Bus::Session,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "ListNames",
                ().to_variant(),
                false,
            )?,
            "ListNames",
        )?;
        names.sort();
        let mut players = Vec::new();
        for service in names
            .into_iter()
            .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
        {
            let Ok(p) = get_all(self.0, Bus::Session, &service, PATH, PLAYER) else {
                continue;
            };
            let root = get_all(
                self.0,
                Bus::Session,
                &service,
                PATH,
                "org.mpris.MediaPlayer2",
            )
            .unwrap_or_default();
            let m: Properties = optional(&p, "Metadata").unwrap_or_default();
            players.push(Player {
                identity: optional(&root, "Identity").unwrap_or_else(|| {
                    service
                        .trim_start_matches("org.mpris.MediaPlayer2.")
                        .to_string()
                }),
                status: optional(&p, "PlaybackStatus").unwrap_or_else(|| "Stopped".into()),
                title: optional(&m, "xesam:title").unwrap_or_default(),
                artists: optional(&m, "xesam:artist").unwrap_or_default(),
                album: optional(&m, "xesam:album").unwrap_or_default(),
                art_url: optional::<String>(&m, "mpris:artUrl").filter(|s| !s.is_empty()),
                track_id: optional::<glib::variant::ObjectPath>(&m, "mpris:trackid")
                    .map(|p| p.to_string()),
                length: optional::<i64>(&m, "mpris:length").unwrap_or(0).max(0),
                position: optional::<i64>(&p, "Position").unwrap_or(0).max(0),
                volume: optional(&p, "Volume").unwrap_or(1.),
                can_control: optional(&p, "CanControl").unwrap_or(false),
                can_play: optional(&p, "CanPlay").unwrap_or(false),
                can_pause: optional(&p, "CanPause").unwrap_or(false),
                can_next: optional(&p, "CanGoNext").unwrap_or(false),
                can_previous: optional(&p, "CanGoPrevious").unwrap_or(false),
                can_seek: optional(&p, "CanSeek").unwrap_or(false),
                service,
            });
        }
        Ok(players)
    }
    pub fn current(&self, preferred: Option<&str>) -> Result<Option<Player>> {
        let mut players = self.players()?;
        players.sort_by_key(|p| {
            (
                !p.playing(),
                preferred != Some(p.service.as_str()),
                p.status != "Paused",
                p.service.clone(),
            )
        });
        Ok(players.into_iter().next())
    }
    pub fn command(&self, p: &Player, command: Command) -> Result<()> {
        if !p.service.starts_with("org.mpris.MediaPlayer2.") || !gio::dbus_is_name(&p.service) {
            return Err(SystemError::invalid("media", "Invalid player service"));
        }
        let (method, args, allowed) = match command {
            Command::Play => ("Play", ().to_variant(), p.can_play),
            Command::Pause => ("Pause", ().to_variant(), p.can_pause),
            Command::PlayPause => ("PlayPause", ().to_variant(), p.can_toggle()),
            Command::Stop => ("Stop", ().to_variant(), true),
            Command::Next => ("Next", ().to_variant(), p.can_next),
            Command::Previous => ("Previous", ().to_variant(), p.can_previous),
            Command::Seek(offset) => ("Seek", (offset,).to_variant(), p.can_seek),
            Command::SetPosition(position) => {
                if position < 0 || (p.length > 0 && position > p.length) {
                    return Err(SystemError::invalid(
                        "media seek",
                        "Position is outside the current track",
                    ));
                }
                let track = p
                    .track_id
                    .as_deref()
                    .filter(|id| *id != "/org/mpris/MediaPlayer2/TrackList/NoTrack")
                    .and_then(|id| glib::variant::ObjectPath::try_from(id).ok())
                    .ok_or_else(|| SystemError::invalid("media seek", "No current track"))?;
                ("SetPosition", (track, position).to_variant(), p.can_seek)
            }
            Command::Volume(v) => {
                if !v.is_finite() || v < 0. {
                    return Err(SystemError::invalid(
                        "media volume",
                        "Volume must be finite and nonnegative",
                    ));
                }
                if !p.can_control {
                    return Err(SystemError::new(
                        ErrorKind::Unsupported,
                        "media",
                        "Player cannot be controlled",
                    ));
                }
                return unit(
                    self.0.call(
                        Bus::Session,
                        &p.service,
                        PATH,
                        "org.freedesktop.DBus.Properties",
                        "Set",
                        (PLAYER, "Volume", v.to_variant()).to_variant(),
                        false,
                    )?,
                    "media volume",
                );
            }
        };
        if !p.can_control || !allowed {
            return Err(SystemError::new(
                ErrorKind::Unsupported,
                "media",
                "The player does not support this action",
            ));
        }
        unit(
            self.0
                .call(Bus::Session, &p.service, PATH, PLAYER, method, args, false)?,
            method,
        )
    }
}
