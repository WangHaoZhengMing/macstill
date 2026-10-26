//! In-memory notification server. Bus and image work belong to its own GLib thread.
mod model;
mod service;
pub use model::{Notification, Snapshot};
use std::sync::{Arc, Mutex, mpsc};
pub enum Command {
    Dismiss(u32),
    Clear,
    Invoke(u32, String),
    Focus(bool),
    Center(bool),
    Stop,
}
pub struct Notifications {
    state: Arc<Mutex<Snapshot>>,
    tx: mpsc::Sender<Command>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Notifications {
    pub fn new(serve: bool) -> Self {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let shared = state.clone();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new().name("macstill-notifications".into()).spawn(move || {
            if let Err(error) = service::run(shared.clone(), rx, serve) {
                let mut state = shared.lock().unwrap();
                state.available = false;
                for notification in &mut state.notifications {
                    notification.active = false;
                    notification.actions.clear();
                    notification.banner_until = None;
                    notification.deadline = None;
                }
                state.diagnostic = Some(format!("Notification service unavailable: {error}"));
                state.revision = state.revision.wrapping_add(1);
                // Never log application names, titles, message bodies, or hints.
                eprintln!("Macstill notification service unavailable; see Notification Center for details.");
            }
        }).expect("notification worker");
        Self {
            state,
            tx,
            thread: Some(thread),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }
    /// Used by protocol tests to prove the D-Bus name is released on shutdown.
    pub fn shutdown(mut self) {
        self.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Notifications {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
    }
}

#[cfg(test)]
impl Notifications {
    pub(crate) fn fixture() -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel();
        let mut model = model::Model::default();
        model.snapshot.available = true;
        for (app, summary, body, urgency, age) in [
            (
                "Mail",
                "Your weekend plans",
                "Alex Chen\nThe table is booked for Saturday at 7. See you there!",
                1,
                25,
            ),
            (
                "Calendar",
                "Design review",
                "Today, 3:00–3:30 PM\nStudio · Project Sonoma",
                1,
                240,
            ),
            (
                "Messages",
                "Emily Parker",
                "Just sent you the photos from our trip. Let me know which ones you like!",
                1,
                720,
            ),
            (
                "System Settings",
                "Your battery is running low",
                "Connect your computer to a power source.",
                2,
                3600,
            ),
        ] {
            let id = model
                .notify(
                    model::Incoming {
                        owner: ":fixture".into(),
                        replaces: 0,
                        app: app.into(),
                        summary: summary.into(),
                        body: body.into(),
                        icon: std::fs::read(macstill_ui::asset(match app {
                            "Mail" => "mail.png",
                            "Calendar" => "calendar.png",
                            "Messages" => "messages.png",
                            _ => "settings.png",
                        }))
                        .ok()
                        .map(Arc::new),
                        actions: if matches!(app, "Mail" | "Messages") {
                            vec![
                                ("default".into(), "Open".into()),
                                ("reply".into(), "Reply".into()),
                            ]
                        } else {
                            vec![("default".into(), "Open".into())]
                        },
                        urgency,
                        resident: false,
                        transient: false,
                        timeout: 0,
                    },
                    std::time::Instant::now(),
                )
                .unwrap();
            model
                .snapshot
                .notifications
                .iter_mut()
                .find(|n| n.id == id)
                .unwrap()
                .received = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        }
        model.snapshot.notifications.reverse();
        (
            Self {
                state: Arc::new(Mutex::new(model.snapshot)),
                tx,
                thread: None,
            },
            rx,
        )
    }
}
