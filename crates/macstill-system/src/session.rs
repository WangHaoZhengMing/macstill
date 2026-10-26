//! logind capability checks, inhibitors and actions using ordinary logind policy.
use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::{ObjectPath, ToVariant};
const SERVICE: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
pub struct SessionSettings<'a, T: Transport>(pub &'a T);
impl PowerAction {
    pub const ALL: [Self; 6] = [
        Self::PowerOff,
        Self::Reboot,
        Self::Suspend,
        Self::Hibernate,
        Self::HybridSleep,
        Self::SuspendThenHibernate,
    ];
    fn method(self) -> &'static str {
        match self {
            Self::PowerOff => "PowerOff",
            Self::Reboot => "Reboot",
            Self::Suspend => "Suspend",
            Self::Hibernate => "Hibernate",
            Self::HybridSleep => "HybridSleep",
            Self::SuspendThenHibernate => "SuspendThenHibernate",
        }
    }
}
impl<T: Transport> SessionSettings<'_, T> {
    fn own_session(&self) -> Result<ObjectPath> {
        let (session,): (ObjectPath,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                PATH,
                MANAGER,
                "GetSessionByPID",
                (std::process::id(),).to_variant(),
                false,
            )?,
            "GetSessionByPID",
        )?;
        path(session.as_str(), "/org/freedesktop/login1/session/")
    }
    pub fn snapshot(&self) -> Result<SessionState> {
        let session = self.own_session()?;
        let p = get_all(self.0, Bus::System, SERVICE, session.as_str(), SESSION)?;
        let (user_id, _): (u32, ObjectPath) = field(&p, "User")?;
        Ok(SessionState {
            id: field(&p, "Id")?,
            user_id,
            user_name: field(&p, "Name")?,
            kind: field(&p, "Type")?,
            desktop: optional(&p, "Desktop").unwrap_or_default(),
            state: field(&p, "State")?,
            active: field(&p, "Active")?,
            locked: field(&p, "LockedHint")?,
            idle: field(&p, "IdleHint")?,
        })
    }
    pub fn capability(&self, action: PowerAction) -> Result<String> {
        let method = format!("Can{}", action.method());
        decode::<(String,)>(
            self.0.call(
                Bus::System,
                SERVICE,
                PATH,
                MANAGER,
                &method,
                ().to_variant(),
                false,
            )?,
            &method,
        )
        .map(|v| v.0)
    }
    pub fn inhibitors(&self) -> Result<Vec<Inhibitor>> {
        type Entry = (String, String, String, String, u32, u32);
        let (entries,): (Vec<Entry>,) = decode(
            self.0.call(
                Bus::System,
                SERVICE,
                PATH,
                MANAGER,
                "ListInhibitors",
                ().to_variant(),
                false,
            )?,
            "ListInhibitors",
        )?;
        Ok(entries
            .into_iter()
            .map(|(what, who, why, mode, uid, pid)| Inhibitor {
                what,
                who,
                why,
                mode,
                uid,
                pid,
            })
            .collect())
    }
    /// Even unsupported actions get a separate error without hiding other capabilities.
    pub fn power_actions(&self) -> Result<PowerActions> {
        Ok(PowerActions {
            capabilities: PowerAction::ALL
                .into_iter()
                .map(|action| PowerCapability {
                    action,
                    availability: self.capability(action).into(),
                })
                .collect(),
            inhibitors: self.inhibitors().into(),
        })
    }
    /// Actual power operation, with ordinary logind policy and inhibitor handling.
    pub fn power_action(&self, action: PowerAction) -> Result<()> {
        let method = action.method();
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                PATH,
                MANAGER,
                method,
                (true,).to_variant(),
                true,
            )?,
            method,
        )
    }
    /// Requests the locker registered with the current session. Acceptance does
    /// not prove that a screen locker is installed or that the screen is locked.
    pub fn lock(&self) -> Result<()> {
        let session = self.own_session()?;
        unit(
            self.0.call(
                Bus::System,
                SERVICE,
                session.as_str(),
                SESSION,
                "Lock",
                ().to_variant(),
                true,
            )?,
            "Lock",
        )
    }
}
