use serde::{Deserialize, Serialize};
use std::fmt;
pub type Result<T> = std::result::Result<T, SystemError>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Unavailable,
    PermissionDenied,
    AuthenticationRequired,
    InvalidInput,
    NotFound,
    Busy,
    Timeout,
    Unsupported,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemError {
    pub kind: ErrorKind,
    pub operation: String,
    pub message: String,
}
impl SystemError {
    pub fn new(kind: ErrorKind, operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            operation: operation.into(),
            message: message.into(),
        }
    }
    pub fn invalid(operation: &str, message: &str) -> Self {
        Self::new(ErrorKind::InvalidInput, operation, message)
    }
    pub(crate) fn protocol(operation: &str) -> Self {
        Self::new(
            ErrorKind::Failed,
            operation,
            "The service returned an unexpected response",
        )
    }
    pub(crate) fn io(operation: &str, e: std::io::Error) -> Self {
        let kind = match e.kind() {
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            std::io::ErrorKind::NotFound => ErrorKind::Unavailable,
            _ => ErrorKind::Failed,
        };
        Self::new(kind, operation, e.to_string())
    }
    pub(crate) fn dbus(operation: &str, e: gio::glib::Error) -> Self {
        let name = gio::DBusError::remote_error(&e)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let kind = if name.ends_with("ServiceUnknown") || name.ends_with("NameHasNoOwner") {
            ErrorKind::Unavailable
        } else if name.contains("NotAuthorized")
            || name.ends_with("AccessDenied")
            || name.ends_with("PermissionDenied")
        {
            ErrorKind::PermissionDenied
        } else if name.contains("NoSecret")
            || name.contains("Authentication")
            || name.contains("InteractiveAuthorizationRequired")
        {
            ErrorKind::AuthenticationRequired
        } else if name.contains("UnknownMethod")
            || name.contains("UnknownInterface")
            || name.contains("NotSupported")
        {
            ErrorKind::Unsupported
        } else if name.contains("UnknownObject")
            || name.contains("DoesNotExist")
            || name.ends_with(".NoSessionForPID")
            || name.ends_with(".NoSuchSession")
        {
            ErrorKind::NotFound
        } else if name.contains("InProgress") || name.contains("Busy") {
            ErrorKind::Busy
        } else if name.contains("NoReply")
            || name.contains("Timeout")
            || e.matches(gio::IOErrorEnum::TimedOut)
        {
            ErrorKind::Timeout
        } else {
            ErrorKind::Failed
        };
        Self::new(
            kind,
            operation,
            if name.is_empty() {
                e.to_string()
            } else {
                format!("{name}: {e}")
            },
        )
    }
}
impl fmt::Display for SystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.operation, self.message)
    }
}
impl std::error::Error for SystemError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logind_missing_session_is_not_found_and_retains_the_remote_reason() {
        for name in [
            "org.freedesktop.login1.NoSessionForPID",
            "org.freedesktop.login1.NoSuchSession",
        ] {
            let error = SystemError::dbus(
                "session",
                gio::DBusError::new_for_dbus_error(name, "No login session"),
            );
            assert_eq!(error.kind, ErrorKind::NotFound);
            assert!(error.message.contains(name));
        }
    }
}
