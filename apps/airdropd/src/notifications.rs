//! Actionable desktop notifications; receiving never needs an open Finder window.
use super::*;
use futures_util::StreamExt;
use std::collections::HashSet;

#[derive(Default)]
struct Bridge {
    owner: String,
    displayed: HashSet<(String, String)>,
    ids: HashMap<u32, (String, String)>,
}
impl Bridge {
    async fn action(
        &self,
        id: u32,
        action: &str,
        state: &Shared,
        commands: &mpsc::Sender<Command>,
    ) {
        let Some((transfer, phase)) = self.ids.get(&id) else {
            return;
        };
        if phase == "pending" && ["accept", "decline"].contains(&action) {
            let pending = state
                .lock()
                .unwrap()
                .transfers
                .iter()
                .any(|t| &t.id == transfer && t.state == "pending");
            if pending {
                let _ = commands
                    .send(Command::Answer {
                        id: transfer.clone(),
                        accept: action == "accept",
                    })
                    .await;
            }
        } else if action == "show" && phase == "completed" {
            let files = state
                .lock()
                .unwrap()
                .transfers
                .iter()
                .find(|t| &t.id == transfer)
                .map(|t| t.files.clone());
            if let Some(files) = files {
                tokio::task::spawn_blocking(move || {
                    let _ = macstill_airdrop::show_received(&files);
                });
            }
        }
    }
    async fn closed(&mut self, id: u32, reason: u32, commands: &mpsc::Sender<Command>) {
        if let Some((transfer, phase)) = self.ids.remove(&id) {
            // Only explicit user dismissal rejects an offer. Server expiry/withdrawal
            // must not turn a previously accepted transfer into a rejection.
            if phase == "pending" && reason == 2 {
                let _ = commands
                    .send(Command::Answer {
                        id: transfer,
                        accept: false,
                    })
                    .await;
            }
        }
    }
    async fn update(&mut self, state: &Shared, proxy: &zbus::Proxy<'_>, owners: &zbus::Proxy<'_>) {
        let owner: String = owners
            .call("GetNameOwner", &("org.freedesktop.Notifications",))
            .await
            .unwrap_or_default();
        if owner != self.owner {
            if !self.owner.is_empty() {
                self.displayed.clear();
                self.ids.clear();
            }
            self.owner = owner;
        }
        let transfers = state.lock().unwrap().transfers.clone();
        let obsolete: Vec<_> = self
            .ids
            .iter()
            .filter(|(_, (id, phase))| {
                !transfers.iter().any(|t| &t.id == id)
                    || (phase == "pending"
                        && !transfers
                            .iter()
                            .any(|t| &t.id == id && t.state == "pending"))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in obsolete {
            let _: Result<(), _> = proxy.call("CloseNotification", &(id,)).await;
            self.ids.remove(&id);
        }
        for transfer in transfers.iter().filter(|t| {
            t.incoming && ["pending", "completed", "failed"].contains(&t.state.as_str())
        }) {
            let key = (transfer.id.clone(), transfer.state.clone());
            if self.displayed.contains(&key) {
                continue;
            }
            if let Ok(id) = publish(proxy, transfer).await {
                self.ids.insert(id, key.clone());
                self.displayed.insert(key);
            }
        }
        self.displayed
            .retain(|(id, _)| transfers.iter().any(|t| &t.id == id));
        self.ids
            .retain(|_, (id, _)| transfers.iter().any(|t| &t.id == id));
    }
}
async fn publish(proxy: &zbus::Proxy<'_>, transfer: &Transfer) -> zbus::Result<u32> {
    let pending = transfer.state == "pending";
    let title = if pending {
        format!("{} would like to share with you", transfer.peer)
    } else if transfer.state == "completed" {
        "AirDrop complete".into()
    } else {
        "AirDrop failed".into()
    };
    let body = transfer.error.clone().unwrap_or_else(|| {
        transfer
            .files
            .iter()
            .take(4)
            .map(|p| {
                PathBuf::from(p)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    let actions = if pending {
        vec!["accept", "Accept", "decline", "Decline"]
    } else if transfer.state == "completed"
        && transfer
            .files
            .iter()
            .any(|p| std::path::Path::new(p).is_absolute())
    {
        vec!["show", "Show in Finder"]
    } else {
        vec![]
    };
    let hints = HashMap::from([
        (
            "desktop-entry",
            zbus::zvariant::Value::from("org.macstill.AirDrop"),
        ),
        ("resident", zbus::zvariant::Value::from(true)),
    ]);
    // The banner may disappear, but its actions remain valid in notification history.
    proxy
        .call(
            "Notify",
            &(
                "AirDrop", 0u32, "airdrop", title, body, actions, hints, 0i32,
            ),
        )
        .await
}
pub(super) async fn run(state: Shared, commands: mpsc::Sender<Command>, bus: zbus::Connection) {
    let Ok(proxy) = zbus::Proxy::new(
        &bus,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
    else {
        return;
    };
    let Ok(mut actions) = proxy.receive_signal("ActionInvoked").await else {
        return;
    };
    let Ok(mut closed) = proxy.receive_signal("NotificationClosed").await else {
        return;
    };
    let Ok(owners) = zbus::Proxy::new(
        &bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await
    else {
        return;
    };
    let mut bridge = Bridge::default();
    let mut timer = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            signal = closed.next() => {
                let Some(signal) = signal else { break; };
                if let Ok((id, reason)) = signal.body().deserialize::<(u32, u32)>() {
                    bridge.closed(id, reason, &commands).await;
                }
            }
            signal = actions.next() => {
                let Some(signal) = signal else { break; };
                if let Ok((id, action)) = signal.body().deserialize::<(u32, String)>() {
                    bridge.action(id, &action, &state, &commands).await;
                }
            }
            _ = timer.tick() => bridge.update(&state, &proxy, &owners).await,
        }
    }
}
