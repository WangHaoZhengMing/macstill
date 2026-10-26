use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::ToVariant;
const HOST: &str = "org.freedesktop.hostname1";
const HOST_PATH: &str = "/org/freedesktop/hostname1";
const TIME: &str = "org.freedesktop.timedate1";
const TIME_PATH: &str = "/org/freedesktop/timedate1";
pub struct HostSettings<'a, T: Transport>(pub &'a T);
impl<T: Transport> HostSettings<'_, T> {
    pub fn snapshot(&self) -> Result<Host> {
        let p = get_all(self.0, Bus::System, HOST, HOST_PATH, HOST)?;
        Ok(Host {
            hostname: field(&p, "Hostname")?,
            static_hostname: field(&p, "StaticHostname")?,
            pretty_hostname: field(&p, "PrettyHostname")?,
            operating_system: optional(&p, "OperatingSystemPrettyName").unwrap_or_default(),
            kernel: optional(&p, "KernelRelease").unwrap_or_default(),
        })
    }
    pub fn date_time(&self) -> Result<DateTime> {
        let p = get_all(self.0, Bus::System, TIME, TIME_PATH, TIME)?;
        Ok(DateTime {
            timezone: field(&p, "Timezone")?,
            ntp: field(&p, "NTP")?,
            can_ntp: field(&p, "CanNTP")?,
            synchronized: field(&p, "NTPSynchronized")?,
            local_rtc: optional(&p, "LocalRTC"),
            unix_microseconds: optional(&p, "TimeUSec"),
            rtc_microseconds: optional(&p, "RTCTimeUSec"),
        })
    }
    pub fn timezones(&self) -> Result<Vec<String>> {
        decode::<(Vec<String>,)>(
            self.0.call(
                Bus::System,
                TIME,
                TIME_PATH,
                TIME,
                "ListTimezones",
                ().to_variant(),
                false,
            )?,
            "ListTimezones",
        )
        .map(|v| v.0)
    }
    /// Absolute UTC microseconds since the Unix epoch. NTP must already be off;
    /// the API does not silently change synchronization policy.
    pub fn set_time(&self, unix_microseconds: i64) -> Result<()> {
        if unix_microseconds < 0 {
            return Err(SystemError::invalid(
                "time",
                "Use nonnegative UTC microseconds since the Unix epoch",
            ));
        }
        unit(
            self.0.call(
                Bus::System,
                TIME,
                TIME_PATH,
                TIME,
                "SetTime",
                (unix_microseconds, false, true).to_variant(),
                true,
            )?,
            "SetTime",
        )
    }
    /// Keep the system clock authoritative; never read the RTC into it.
    pub fn set_local_rtc(&self, local: bool) -> Result<()> {
        unit(
            self.0.call(
                Bus::System,
                TIME,
                TIME_PATH,
                TIME,
                "SetLocalRTC",
                (local, false, true).to_variant(),
                true,
            )?,
            "SetLocalRTC",
        )
    }
    pub fn set_timezone(&self, timezone: &str) -> Result<()> {
        if timezone.is_empty()
            || timezone.starts_with('/')
            || timezone
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
            || !timezone
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/_+-".contains(&c))
        {
            return Err(SystemError::invalid(
                "timezone",
                "Choose an IANA timezone, for example Asia/Shanghai",
            ));
        }
        // timedated validates membership in the system timezone database.
        unit(
            self.0.call(
                Bus::System,
                TIME,
                TIME_PATH,
                TIME,
                "SetTimezone",
                (timezone, true).to_variant(),
                true,
            )?,
            "SetTimezone",
        )
    }
    pub fn set_ntp(&self, enabled: bool) -> Result<()> {
        unit(
            self.0.call(
                Bus::System,
                TIME,
                TIME_PATH,
                TIME,
                "SetNTP",
                (enabled, true).to_variant(),
                true,
            )?,
            "SetNTP",
        )
    }
    /// Sets the persistent hostname; DHCP may continue to supply a different transient hostname.
    pub fn set_hostname(&self, hostname: &str) -> Result<()> {
        if !valid_hostname(hostname) {
            return Err(SystemError::invalid(
                "hostname",
                "Use DNS labels containing lowercase letters, numbers and internal hyphens (64 characters maximum)",
            ));
        }
        unit(
            self.0.call(
                Bus::System,
                HOST,
                HOST_PATH,
                HOST,
                "SetStaticHostname",
                (hostname, true).to_variant(),
                true,
            )?,
            "SetStaticHostname",
        )
    }
    pub fn set_pretty_hostname(&self, hostname: &str) -> Result<()> {
        if hostname.len() > 256 || hostname.chars().any(char::is_control) {
            return Err(SystemError::invalid(
                "computer name",
                "Use at most 256 UTF-8 bytes without control characters",
            ));
        }
        unit(
            self.0.call(
                Bus::System,
                HOST,
                HOST_PATH,
                HOST,
                "SetPrettyHostname",
                (hostname, true).to_variant(),
                true,
            )?,
            "SetPrettyHostname",
        )
    }
}
pub fn valid_hostname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}
