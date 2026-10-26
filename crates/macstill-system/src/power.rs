use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::ToVariant;
pub struct Power<'a, T: Transport>(pub &'a T);
impl<T: Transport> Power<'_, T> {
    pub fn snapshot(&self) -> Result<PowerState> {
        let p = get_all(
            self.0,
            Bus::System,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )?;
        let battery = get_all(
            self.0,
            Bus::System,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower/devices/DisplayDevice",
            "org.freedesktop.UPower.Device",
        )?;
        Ok(PowerState {
            on_battery: field(&p, "OnBattery")?,
            battery: if field::<bool>(&battery, "IsPresent")? {
                Some(Battery {
                    percentage: field(&battery, "Percentage")?,
                    state: field(&battery, "State")?,
                    time_to_empty_seconds: field(&battery, "TimeToEmpty")?,
                    time_to_full_seconds: field(&battery, "TimeToFull")?,
                })
            } else {
                None
            },
        })
    }
    fn profile_endpoint(&self) -> Result<(&'static str, &'static str, Properties)> {
        // New daemons use UPower; distributions with the original name remain supported.
        let service = "org.freedesktop.UPower.PowerProfiles";
        let path = "/org/freedesktop/UPower/PowerProfiles";
        match get_all(self.0, Bus::System, service, path, service) {
            Ok(p) => Ok((service, path, p)),
            Err(e)
                if matches!(
                    e.kind,
                    ErrorKind::Unavailable | ErrorKind::NotFound | ErrorKind::Unsupported
                ) =>
            {
                let service = "net.hadess.PowerProfiles";
                let path = "/net/hadess/PowerProfiles";
                get_all(self.0, Bus::System, service, path, service).map(|p| (service, path, p))
            }
            Err(e) => Err(e),
        }
    }
    pub fn profiles(&self) -> Result<PowerProfiles> {
        let (_, _, p) = self.profile_endpoint()?;
        Self::parse_profiles(&p)
    }
    fn parse_profiles(p: &Properties) -> Result<PowerProfiles> {
        let profiles: Vec<Properties> = field(p, "Profiles")?;
        let available: Result<Vec<String>> = profiles.iter().map(|p| field(p, "Profile")).collect();
        Ok(PowerProfiles {
            active: field(p, "ActiveProfile")?,
            available: available?,
            performance_degraded: optional(p, "PerformanceDegraded").unwrap_or_default(),
        })
    }
    pub fn set_profile(&self, profile: &str) -> Result<()> {
        let (service, path, p) = self.profile_endpoint()?;
        if !Self::parse_profiles(&p)?
            .available
            .iter()
            .any(|v| v == profile)
        {
            return Err(SystemError::invalid(
                "power profile",
                "Select a profile supported by this machine",
            ));
        }
        set(
            self.0,
            Bus::System,
            service,
            path,
            service,
            "ActiveProfile",
            profile.to_variant(),
        )
    }
}

impl<T: Transport> Power<'_, T> {
    /// Read health and ten days of charge history. Peripheral batteries are excluded.
    pub fn battery_report(&self) -> Result<BatteryReport> {
        let service = "org.freedesktop.UPower";
        let interface = "org.freedesktop.UPower.Device";
        let devices = self.0.call(
            Bus::System,
            service,
            "/org/freedesktop/UPower",
            service,
            "EnumerateDevices",
            ().to_variant(),
            false,
        )?;
        let (mut paths,) = devices
            .get::<(Vec<gio::glib::variant::ObjectPath>,)>()
            .ok_or_else(|| SystemError::protocol("UPower.EnumerateDevices"))?;
        paths.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut batteries = Vec::new();
        for path in paths.into_iter().take(32) {
            if !path
                .as_str()
                .starts_with("/org/freedesktop/UPower/devices/")
            {
                continue;
            }
            let Ok(props) = get_all(self.0, Bus::System, service, path.as_str(), interface) else {
                continue;
            };
            if optional::<u32>(&props, "Type") == Some(2)
                && optional::<bool>(&props, "PowerSupply") == Some(true)
                && optional::<bool>(&props, "IsPresent") == Some(true)
            {
                batteries.push((path, props));
            }
        }
        let count = batteries.len();
        let (path, props) = batteries.into_iter().next().ok_or_else(|| {
            SystemError::new(
                ErrorKind::NotFound,
                "Battery health",
                "No system battery was found",
            )
        })?;
        let positive = |key| optional::<f64>(&props, key).filter(|v| v.is_finite() && *v > 0.);
        let mut report = BatteryReport {
            device: path
                .as_str()
                .rsplit('/')
                .next()
                .unwrap_or("Battery")
                .trim_start_matches("battery_")
                .to_string(),
            capacity_percent: positive("Capacity").filter(|v| *v <= 100.),
            charge_cycles: optional::<i32>(&props, "ChargeCycles").filter(|v| *v >= 0),
            energy_full_wh: positive("EnergyFull"),
            energy_design_wh: positive("EnergyFullDesign"),
            history: vec![],
            history_error: None,
            battery_count: count,
        };
        if optional::<bool>(&props, "HasHistory") != Some(true) {
            report.history_error = Some("This battery does not provide charge history.".into());
            return Ok(report);
        }
        let history = self
            .0
            .call(
                Bus::System,
                service,
                path.as_str(),
                interface,
                "GetHistory",
                ("charge", 864000u32, 2400u32).to_variant(),
                false,
            )
            .and_then(|v| {
                v.get::<(Vec<(u32, f64, u32)>,)>()
                    .map(|v| v.0)
                    .ok_or_else(|| SystemError::protocol("UPower.GetHistory"))
            });
        match history {
            Ok(points) => {
                report.history = points
                    .into_iter()
                    .take(10_000)
                    .filter(|(time, value, state)| {
                        *time > 0
                            && value.is_finite()
                            && (0. ..=100.).contains(value)
                            && (1..=6).contains(state)
                    })
                    .map(|(timestamp, percentage, state)| BatterySample {
                        timestamp,
                        percentage,
                        state,
                    })
                    .collect();
                report.history.sort_by_key(|point| point.timestamp);
                report.history.dedup_by_key(|point| point.timestamp);
            }
            Err(error) => report.history_error = Some(error.to_string()),
        }
        Ok(report)
    }
}
