use super::{
    Command, Snapshot,
    model::{Incoming, Model, Signal, plain_text},
};
use gio::{
    glib::{self, variant::ToVariant},
    prelude::*,
};
use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const XML: &str = r#"<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="GetServerInformation"><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/></method>
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
</interface></node>"#;
type NotifyArgs = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, glib::Variant>,
    i32,
);
fn bus_call(
    c: &gio::DBusConnection,
    method: &str,
    args: glib::Variant,
) -> Result<glib::Variant, glib::Error> {
    c.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        method,
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        1500,
        gio::Cancellable::NONE,
    )
}
fn emit(c: &gio::DBusConnection, signals: Vec<Signal>) {
    for signal in signals {
        let (name, args) = match signal {
            Signal::Closed(id, reason) => ("NotificationClosed", (id, reason).to_variant()),
            Signal::Action(id, key) => ("ActionInvoked", (id, key).to_variant()),
        };
        let _ = c.emit_signal(None, PATH, NAME, name, Some(&args));
    }
}
pub(super) fn run(
    shared: Arc<Mutex<Snapshot>>,
    rx: mpsc::Receiver<Command>,
    serve: bool,
) -> anyhow::Result<()> {
    let context = glib::MainContext::new();
    context.with_thread_default(|| connected(&context, shared, rx, serve))?
}
fn connected(
    context: &glib::MainContext,
    shared: Arc<Mutex<Snapshot>>,
    rx: mpsc::Receiver<Command>,
    serve: bool,
) -> anyhow::Result<()> {
    let model = Arc::new(Mutex::new(Model::default()));
    let connection = if serve {
        let address =
            gio::dbus_address_get_for_bus_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
        let c = gio::DBusConnection::for_address_sync(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )?;
        c.set_exit_on_close(false);
        Some(c)
    } else {
        None
    };
    let mut registration = None;
    if let Some(c) = &connection {
        let info = gio::DBusNodeInfo::for_xml(XML)?
            .lookup_interface(NAME)
            .unwrap();
        let data = model.clone();
        registration = Some(
            c.register_object(PATH, &info)
                .method_call(move |c, sender, _, _, method, args, inv| match method {
                    "GetCapabilities" => inv.return_value(Some(
                        &(vec!["actions", "body", "icon-static"],).to_variant(),
                    )),
                    "GetServerInformation" => inv.return_value(Some(
                        &("Macstill", "Macstill", env!("CARGO_PKG_VERSION"), "1.3").to_variant(),
                    )),
                    "Notify" => {
                        let Some((app, replaces, icon, summary, body, actions, hints, timeout)) =
                            args.get::<NotifyArgs>()
                        else {
                            inv.return_dbus_error(
                                "org.freedesktop.DBus.Error.InvalidArgs",
                                "Invalid notification arguments",
                            );
                            return;
                        };
                        if actions.len() % 2 != 0
                            || actions.len() > 32
                            || timeout < -1
                            || app.len() > 1024
                            || summary.len() > 8192
                            || body.len() > 65536
                            || icon.len() > 4096
                        {
                            inv.return_dbus_error(
                                "org.freedesktop.DBus.Error.InvalidArgs",
                                "Notification arguments exceed supported limits",
                            );
                            return;
                        }
                        let icon = notification_icon(&icon, &hints);
                        let incoming = Incoming {
                            owner: sender.unwrap_or("").into(),
                            replaces,
                            app: {
                                let name = plain_text(&app, 128);
                                if name.trim().is_empty() {
                                    "Application".into()
                                } else {
                                    name
                                }
                            },
                            summary: plain_text(&summary, 512),
                            body: plain_text(&body, 4096),
                            icon,
                            actions: actions
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .filter(|pair| !pair[0].is_empty() && pair[0].len() <= 256)
                                .map(|pair| (pair[0].clone(), plain_text(&pair[1], 80)))
                                .collect(),
                            urgency: hints
                                .get("urgency")
                                .and_then(|v| v.get::<u8>())
                                .unwrap_or(1)
                                .min(2),
                            resident: hint_bool(&hints, "resident"),
                            transient: hint_bool(&hints, "transient"),
                            timeout,
                        };
                        match data.lock().unwrap().notify(incoming, Instant::now()) {
                            Ok(id) => inv.return_value(Some(&(id,).to_variant())),
                            Err(_) => inv.return_dbus_error(
                                "org.freedesktop.DBus.Error.LimitsExceeded",
                                "Too many active notifications",
                            ),
                        }
                    }
                    "CloseNotification" => {
                        let id = args.child_value(0).get::<u32>().unwrap_or(0);
                        let mut m = data.lock().unwrap();
                        if m.snapshot
                            .notifications
                            .iter()
                            .any(|n| n.id == id && n.active && n.owner == sender.unwrap_or(""))
                        {
                            let signals = m.close(id, 3, true);
                            inv.return_value(None);
                            emit(&c, signals);
                        } else {
                            inv.return_dbus_error(
                                "org.freedesktop.Notifications.Error.InvalidId",
                                "No active notification with this ID for this sender",
                            );
                        }
                    }
                    _ => inv.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "Unknown notification method",
                    ),
                })
                .build()?,
        );
        // DO_NOT_QUEUE only: never request replacement or allow another instance to replace us.
        let result = bus_call(c, "RequestName", (NAME, 4u32).to_variant())?;
        if result.child_value(0).get::<u32>() != Some(1) {
            let owner = bus_call(c, "GetNameOwner", (NAME,).to_variant())
                .ok()
                .and_then(|v| v.child_value(0).get::<String>())
                .unwrap_or_else(|| "another service".into());
            anyhow::bail!(
                "org.freedesktop.Notifications is already owned by {owner}. Macstill did not replace it. Choose one notification server in your session startup; other servers' history cannot be read here."
            );
        }
        model.lock().unwrap().snapshot.available = true;
    } else {
        model.lock().unwrap().snapshot.diagnostic =
            Some("Preview only. Notifications are received by the Macstill shell session.".into());
    }
    model.lock().unwrap().snapshot.revision = 1;
    let mut published = u64::MAX;
    loop {
        // Bound each batch so continuous clients cannot starve UI commands or expiry.
        for _ in 0..32 {
            if !context.pending() {
                break;
            }
            context.iteration(false);
        }
        let mut stop = false;
        for _ in 0..64 {
            let command = match rx.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Disconnected) => {
                    stop = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            };
            let mut m = model.lock().unwrap();
            let signals = match command {
                Command::Stop => {
                    stop = true;
                    vec![]
                }
                Command::Dismiss(id) => m.close(id, 2, true),
                Command::Clear => m.clear(),
                Command::Invoke(id, key) => m.invoke(id, &key),
                Command::Focus(value) => {
                    m.set_focus(value);
                    vec![]
                }
                Command::Center(value) => {
                    m.center(value);
                    vec![]
                }
            };
            if let Some(c) = &connection {
                emit(c, signals);
            }
        }
        if stop {
            break;
        }
        if connection.as_ref().is_some_and(|c| c.is_closed()) {
            anyhow::bail!("Session D-Bus connection closed");
        }
        let mut m = model.lock().unwrap();
        let signals = m.tick(Instant::now());
        if let Some(c) = &connection {
            emit(c, signals);
        }
        if m.snapshot.revision != published {
            published = m.snapshot.revision;
            *shared.lock().unwrap() = m.snapshot.clone();
        }
        drop(m);
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(c) = connection {
        if let Some(registration) = registration {
            let _ = c.unregister_object(registration);
        }
        let _ = bus_call(&c, "ReleaseName", (NAME,).to_variant());
        let _ = c.close_sync(gio::Cancellable::NONE);
    }
    Ok(())
}
fn hint_bool(hints: &HashMap<String, glib::Variant>, key: &str) -> bool {
    hints
        .get(key)
        .and_then(|v| v.get::<bool>())
        .unwrap_or(false)
}
fn png(image: image::DynamicImage) -> Option<Arc<Vec<u8>>> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    let image = if image.width() > 128 || image.height() > 128 {
        image.thumbnail(128, 128)
    } else {
        image
    };
    image.write_to(&mut buffer, image::ImageFormat::Png).ok()?;
    Some(Arc::new(buffer.into_inner()))
}
fn raw_image(value: &glib::Variant) -> Option<Arc<Vec<u8>>> {
    if value.type_().as_str() != "(iiibiiay)"
        || value.child_value(6).fixed_array::<u8>().ok()?.len() > 4 * 1024 * 1024
    {
        return None;
    }
    let (w, h, stride, alpha, bits, channels, bytes) =
        value.get::<(i32, i32, i32, bool, i32, i32, Vec<u8>)>()?;
    if !(1..=512).contains(&w)
        || !(1..=512).contains(&h)
        || bits != 8
        || channels != if alpha { 4 } else { 3 }
        || stride < w * channels
        || stride > 8192
    {
        return None;
    }
    let required = (h as usize - 1) * stride as usize + w as usize * channels as usize;
    if bytes.len() < required || bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h as usize {
        for col in 0..w as usize {
            let offset = row * stride as usize + col * channels as usize;
            rgba.extend_from_slice(&bytes[offset..offset + 3]);
            rgba.push(if alpha { bytes[offset + 3] } else { 255 });
        }
    }
    png(image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(
        w as u32, h as u32, rgba,
    )?))
}
fn file_icon(name: &str) -> Option<Arc<Vec<u8>>> {
    if name.is_empty() || name.len() > 4096 {
        return None;
    }
    let path = if name.starts_with("file://") {
        gio::File::for_uri(name).path()?
    } else if std::path::Path::new(name).is_absolute() {
        name.into()
    } else {
        if name.contains('/') || name.contains(':') {
            return None;
        }
        freedesktop_icons::lookup(name)
            .with_size(64)
            .with_theme("hicolor")
            .with_cache()
            .find()?
    };
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    png(reader.decode().ok()?)
}
fn notification_icon(
    app_icon: &str,
    hints: &HashMap<String, glib::Variant>,
) -> Option<Arc<Vec<u8>>> {
    for key in ["image-data", "image_data"] {
        if let Some(icon) = hints.get(key).and_then(raw_image) {
            return Some(icon);
        }
    }
    for key in ["image-path", "image_path"] {
        if let Some(icon) = hints.get(key).and_then(|v| v.str()).and_then(file_icon) {
            return Some(icon);
        }
    }
    if let Some(icon) = file_icon(app_icon) {
        return Some(icon);
    }
    if let Some(icon) = hints.get("icon_data").and_then(raw_image) {
        return Some(icon);
    }
    let desktop = hints.get("desktop-entry")?.str()?;
    if desktop.len() > 256 || desktop.contains('/') || desktop.contains("..") {
        return None;
    }
    let app =
        gio::DesktopAppInfo::new(&format!("{}.desktop", desktop.trim_end_matches(".desktop")))?;
    let icon = app.icon()?;
    if let Some(file) = icon.downcast_ref::<gio::FileIcon>() {
        return file.file().path().and_then(|p| file_icon(p.to_str()?));
    }
    icon.downcast_ref::<gio::ThemedIcon>()?
        .names()
        .iter()
        .find_map(|name| file_icon(name))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_images_validate_dimensions_stride_alpha_and_padding() {
        assert!(raw_image(&(2, 2, 2, true, 8, 4, vec![0u8; 16]).to_variant()).is_none());
        assert!(raw_image(&(0, 2, 8, true, 8, 4, vec![0u8; 16]).to_variant()).is_none());
        assert!(raw_image(&(2, 2, 8, true, 8, 4, vec![0u8; 15]).to_variant()).is_none());
        let bytes =
            raw_image(&(1, 2, 4, false, 8, 3, vec![255u8, 0, 0, 0, 0, 255, 0]).to_variant())
                .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
        assert_eq!(image.get_pixel(0, 1).0, [0, 255, 0, 255]);
    }
    #[test]
    fn remote_and_non_image_icons_are_rejected() {
        assert!(file_icon("https://example.com/image.png").is_none());
        assert!(file_icon("/etc/passwd").is_none());
        assert!(file_icon("/dev/zero").is_none());
    }
}
