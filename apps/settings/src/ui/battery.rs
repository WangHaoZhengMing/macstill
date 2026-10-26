use super::{widgets::*, *};
use gpui_kit::base::Popover;
use macstill_system::{BatteryReport, BatterySample, PowerProfiles, SystemCommand};
use swiftui_gpui::{settings_divider, settings_group};

impl SettingsView {
    pub(super) fn battery(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = settings_palette(self.snapshot(cx).preferences.dark);
        let state = self.system_state(cx);
        let battery = state
            .system
            .power
            .value
            .as_ref()
            .and_then(|power| power.battery.as_ref());
        let profiles = state.system.power_profiles.value.as_ref();
        let report = state.battery_report.value.as_ref();
        let low_power = profiles.is_some_and(|profiles| profiles.active == "power-saver");
        let mut content = stack().gap(px(10.)).child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(rgb(p.muted))
                .pb(px(2.))
                .child(symbol("battery", 17., p.muted))
                .child(match battery {
                    Some(battery) => {
                        format!("Battery Level: {:.0}%", battery.percentage.clamp(0., 100.))
                    }
                    None if state.system.power.value.is_some() => "No Battery Installed".into(),
                    _ => "Battery information unavailable".into(),
                }),
        );
        let mode = profiles
            .map(|_| if low_power { "Always" } else { "Never" })
            .unwrap_or("Unavailable");
        let weak = cx.entity().downgrade();
        let options = if state.busy || state.loading {
            None
        } else {
            profiles.cloned()
        };
        let low_power_menu = Popover::new("battery-low-power-menu")
            .when(state.busy || state.loading || profiles.is_none(), |menu| {
                menu.open(false)
            })
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .trigger(
                button("battery-low-power", mode, p)
                    .border_0()
                    .bg(transparent_black())
                    .px(px(4.))
                    .py(px(2.))
                    .disabled(state.busy || state.loading || profiles.is_none())
                    .child(
                        div()
                            .ml(px(6.))
                            .w(px(15.))
                            .h(px(20.))
                            .rounded(px(4.))
                            .bg(rgb(p.line))
                            .text_size(px(12.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(chevrons(p)),
                    ),
            )
            .content(move |_, _, cx| {
                let mut menu = div()
                    .w(px(190.))
                    .p(px(5.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(p.line))
                    .bg(rgb(p.surface))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(2.));
                for (label, profile) in [("Never", "balanced"), ("Always", "power-saver")] {
                    let available = options
                        .as_ref()
                        .is_some_and(|p| p.available.iter().any(|a| a == profile));
                    let selected = options.as_ref().is_some_and(|p| p.active == profile);
                    let view = weak.clone();
                    menu = menu.child(
                        button(
                            eid("battery-low-power-option", profile),
                            format!("{}  {label}", if selected { "✓" } else { "  " }),
                            p,
                        )
                        .w_full()
                        .border_0()
                        .bg(transparent_black())
                        .justify_start()
                        .disabled(!available)
                        .on_click(cx.listener(
                            move |popover, _, window, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.apply_system(
                                        SystemCommand::SetPowerProfile(profile.into()),
                                        cx,
                                    )
                                });
                                popover.dismiss(window, cx);
                            },
                        )),
                    );
                }
                menu
            });
        content = content.child(
            group(p)
                .p(px(12.))
                .flex()
                .flex_row()
                .items_start()
                .gap(px(12.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child("Low Power Mode")
                        .child(
                            caption(
                                "Your computer will reduce energy usage to increase battery life.",
                                p,
                            )
                            .text_size(px(12.))
                            .max_w(px(350.)),
                        ),
                )
                .child(low_power_menu),
        );
        let health = report
            .and_then(|report| report.capacity_percent)
            .map(|capacity| {
                if capacity >= 80. {
                    "Normal"
                } else {
                    "Reduced Capacity"
                }
            })
            .unwrap_or("Unavailable");
        let health_details = report.cloned();
        let health_info = Popover::new("battery-health-menu")
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .trigger(
                button("battery-health-info", "", p)
                    .accessibility_label("Battery health details")
                    .p(px(0.))
                    .border_0()
                    .bg(transparent_black())
                    .child(symbol("info", 17., p.muted)),
            )
            .content(move |_, _, _| health_card(health_details.as_ref(), p));
        content = content.child(
            group(p)
                .p(px(12.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().child("Battery Health"))
                .child(caption(health, p))
                .child(health_info),
        );
        let mut tabs = div()
            .w_full()
            .h(px(26.))
            .flex()
            .p(px(1.))
            .rounded(px(6.))
            .bg(rgb(if p.dark { 0x3c3c3e } else { 0xe3e1e2 }));
        for (index, label) in ["Last 24 Hours", "Last 10 Days"].into_iter().enumerate() {
            let days = index == 1;
            tabs = tabs.child(
                button(("battery-period", index), label, p)
                    .flex_1()
                    .h(px(24.))
                    .py(px(0.))
                    .px(px(2.))
                    .rounded(px(5.))
                    .text_size(px(12.))
                    .border_color(if self.battery_days == days {
                        rgb(p.line).into()
                    } else {
                        transparent_black()
                    })
                    .bg(if self.battery_days == days {
                        rgb(p.surface).into()
                    } else {
                        transparent_black()
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.battery_days = days;
                        cx.notify();
                    })),
            );
        }
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        let points = report
            .map(|report| report.history.as_slice())
            .unwrap_or_default();
        let last_charge = points
            .iter()
            .rev()
            .find(|point| matches!(point.state, 1 | 4));
        let (charge_title, charge_time) = if let Some(point) = last_charge {
            (
                format!("Last charged to {:.0}%", point.percentage),
                chrono::DateTime::from_timestamp(point.timestamp as i64, 0)
                    .map(|time| {
                        time.with_timezone(&chrono::Local)
                            .format("%a, %-d %b, %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default(),
            )
        } else if let Some(battery) = battery {
            let status = match battery.state {
                1 => "Charging",
                4 => "Fully charged",
                _ if state
                    .system
                    .power
                    .value
                    .as_ref()
                    .is_some_and(|power| power.on_battery) =>
                {
                    "Using battery power"
                }
                _ => "Power adapter connected",
            };
            (
                status.into(),
                if battery.time_to_full_seconds > 0 {
                    format!(
                        "{} until fully charged",
                        duration_label(battery.time_to_full_seconds)
                    )
                } else if battery.time_to_empty_seconds > 0 {
                    format!(
                        "About {} remaining",
                        duration_label(battery.time_to_empty_seconds)
                    )
                } else {
                    "No recent charging information".into()
                },
            )
        } else {
            (
                "Battery Usage".into(),
                "History appears when a battery is connected.".into(),
            )
        };
        let mut history = group(p)
            .p(px(11.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(tabs)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(charge_title)
                    .child(caption(charge_time, p).text_size(px(11.))),
            )
            .child(settings_divider(p))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Battery Level"),
            )
            .child(chart(points, self.battery_days, now, false, p))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Screen On Usage"),
            )
            .child(chart(&[], self.battery_days, now, true, p));
        if points.is_empty() {
            history = history.child(
                caption(
                    report
                        .and_then(|r| r.history_error.clone())
                        .unwrap_or_else(|| "No battery history available yet.".into()),
                    p,
                )
                .text_size(px(11.)),
            );
        } else if report.is_some_and(|r| r.battery_count > 1) {
            history = history.child(
                caption(format!("Charge history for {}", report.unwrap().device), p)
                    .text_size(px(11.)),
            );
        }
        content = content.child(history);
        if let Some(profiles) = profiles {
            content = content.child(
                div()
                    .flex()
                    .justify_end()
                    .child(self.battery_options(profiles, state.busy, p, cx)),
            );
            if !profiles.performance_degraded.is_empty() {
                content = content.child(caption(profiles.performance_degraded.clone(), p));
            }
        }
        content.into_any_element()
    }
    fn battery_options(
        &self,
        profiles: &PowerProfiles,
        busy: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Popover {
        let profiles = profiles.clone();
        let view = cx.entity().downgrade();
        Popover::new("battery-options-menu")
            .when(busy, |menu| menu.open(false))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .trigger(
                button("battery-options", "Options…", p)
                    .text_size(px(11.))
                    .py(px(3.))
                    .disabled(busy),
            )
            .content(move |_, _, cx| {
                let mut menu = div()
                    .w(px(235.))
                    .p(px(10.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgb(p.line))
                    .bg(rgb(p.surface))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Energy Mode"));
                for profile in profiles.available {
                    let label = match profile.as_str() {
                        "power-saver" => "Low Power",
                        "balanced" => "Automatic",
                        "performance" => "High Performance",
                        _ => profile.as_str(),
                    };
                    let label = format!(
                        "{}  {label}",
                        if profile == profiles.active {
                            "✓"
                        } else {
                            "  "
                        }
                    );
                    let view = view.clone();
                    menu = menu.child(
                        button(eid("battery-profile", &profile), label, p)
                            .w_full()
                            .border_0()
                            .bg(transparent_black())
                            .justify_start()
                            .disabled(busy)
                            .on_click(cx.listener(move |popover, _, window, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.apply_system(
                                        SystemCommand::SetPowerProfile(profile.clone()),
                                        cx,
                                    )
                                });
                                popover.dismiss(window, cx);
                            })),
                    );
                }
                menu.child(
                    caption("Low Power Mode applies to the current power profile.", p)
                        .text_size(px(11.)),
                )
            })
    }
}
fn chevrons(p: Palette) -> AnyElement {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="17" viewBox="0 0 12 17"><path d="m3 6 3-3 3 3m-6 5 3 3 3-3" fill="none" stroke="#{:06x}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
        p.text
    );
    img(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.into_bytes(),
    )))
    .w(px(12.))
    .h(px(17.))
    .into_any_element()
}
fn group(p: Palette) -> Div {
    settings_group(p).bg(rgb(if p.dark { 0x2d2d30 } else { 0xf1eff0 }))
}
fn health_card(report: Option<&BatteryReport>, p: Palette) -> Div {
    let mut card = div()
        .w(px(280.))
        .p(px(15.))
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(p.line))
        .bg(rgb(p.surface))
        .shadow_lg()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Battery Health"),
        );
    let Some(report) = report else {
        return card.child(caption(
            "Battery health information is not available for this computer.",
            p,
        ));
    };
    for (title, value) in [
        (
            "Maximum Capacity",
            report.capacity_percent.map(|v| format!("{v:.0}%")),
        ),
        ("Cycle Count", report.charge_cycles.map(|v| v.to_string())),
        (
            "Full Charge Capacity",
            report.energy_full_wh.map(|v| format!("{v:.1} Wh")),
        ),
        (
            "Design Capacity",
            report.energy_design_wh.map(|v| format!("{v:.1} Wh")),
        ),
    ] {
        card = card.child(
            div()
                .flex()
                .justify_between()
                .gap(px(12.))
                .text_size(px(12.))
                .child(title)
                .child(caption(value.unwrap_or_else(|| "Not reported".into()), p)),
        );
    }
    card.child(caption("Maximum capacity measures how much charge your battery can hold compared with when it was new.",p).text_size(px(11.)))
}
fn bins(points: &[BatterySample], start: u64, span: u64, count: usize) -> Vec<Option<(f64, bool)>> {
    let mut values = vec![(0., 0u32, false); count];
    for point in points {
        let time = point.timestamp as u64;
        if time < start || time > start + span || !point.percentage.is_finite() {
            continue;
        }
        let index =
            (((time - start) as f64 / span as f64 * count as f64).floor() as usize).min(count - 1);
        values[index].0 += point.percentage.clamp(0., 100.);
        values[index].1 += 1;
        values[index].2 |= point.state == 1;
    }
    values
        .into_iter()
        .map(|(sum, count, charging)| (count > 0).then(|| (sum / count as f64, charging)))
        .collect()
}
fn chart(points: &[BatterySample], days: bool, now: u64, screen: bool, p: Palette) -> AnyElement {
    use std::fmt::Write;
    let span = if days { 864000 } else { 86400 };
    let start = now.saturating_sub(span);
    let grid = if p.dark { 0x505054 } else { 0xd0cdcf };
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="126" viewBox="0 0 600 126"><g font-family="sans-serif" font-size="11" fill="#{:06x}">"##,
        p.muted
    );
    for (y, label) in [
        (4, if screen { "60m" } else { "100%" }),
        (47, if screen { "30m" } else { "50%" }),
        (90, if screen { "0m" } else { "0%" }),
    ] {
        let _ = write!(
            svg,
            r##"<path d="M0 {y}H553" fill="none" stroke="#{grid:06x}" stroke-width=".7"/><text x="559" y="{}">{label}</text>"##,
            y + 4
        );
    }
    let ticks = if days { 10 } else { 8 };
    for i in 0..=ticks {
        let x = i as f64 / ticks as f64 * 553.;
        let timestamp = start + span * i / ticks;
        let label = chrono::DateTime::from_timestamp(timestamp as i64, 0)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format(if days { "%-d" } else { "%H" })
                    .to_string()
            })
            .unwrap_or_default();
        let _ = write!(
            svg,
            r##"<path d="M{x:.1} 4V94" fill="none" stroke="#{grid:06x}" stroke-width=".7" stroke-dasharray="2 2"/><text x="{x:.1}" y="113" text-anchor="{}">{label}</text>"##,
            if i == 0 {
                "start"
            } else if i == ticks {
                "end"
            } else {
                "middle"
            }
        );
    }
    let count = if days { 80 } else { 96 };
    let values = bins(points, start, span, count);
    let has_points = values.iter().any(Option::is_some);
    if !screen {
        let width = 553. / count as f64;
        for (i, value) in values.into_iter().enumerate() {
            if let Some((percent, charging)) = value {
                let x = i as f64 * width + 0.6;
                let height = percent / 100. * 86.;
                if charging {
                    let _ = write!(
                        svg,
                        r##"<rect x="{x:.1}" y="4" width="{:.1}" height="86" fill="#24cb38" opacity=".15"/><rect x="{x:.1}" y="95" width="{:.1}" height="3" rx="1.5" fill="#24cb38"/>"##,
                        width - 1.2,
                        width - 1.2
                    );
                }
                let _ = write!(
                    svg,
                    r##"<rect x="{x:.1}" y="{:.1}" width="{:.1}" height="{height:.1}" rx="1.2" fill="#24cb38"/>"##,
                    90. - height,
                    width - 1.2
                );
            }
        }
    }
    if screen || !has_points {
        let _ = write!(
            svg,
            r##"<text x="276" y="49" text-anchor="middle" fill="#{:06x}">{}</text>"##,
            p.muted,
            if screen {
                "Screen-on history is not available"
            } else {
                "No battery history"
            }
        );
    }
    svg.push_str("</g></svg>");
    img(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.into_bytes(),
    )))
    .w_full()
    .h(px(126.))
    .object_fit(ObjectFit::Fill)
    .into_any_element()
}
fn duration_label(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 59) / 60;
    if minutes >= 60 {
        format!("{} hr {} min", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} min")
    }
}
#[cfg(test)]
mod tests {
    use super::{BatterySample, bins};
    #[test]
    fn graph_bins_do_not_invent_history_or_bridge_missing_intervals() {
        let points = vec![
            BatterySample {
                timestamp: 100,
                percentage: 80.,
                state: 2,
            },
            BatterySample {
                timestamp: 110,
                percentage: 60.,
                state: 1,
            },
            BatterySample {
                timestamp: 190,
                percentage: 30.,
                state: 2,
            },
            BatterySample {
                timestamp: 250,
                percentage: 90.,
                state: 1,
            },
        ];
        assert_eq!(
            bins(&points, 100, 100, 4),
            vec![Some((70., true)), None, None, Some((30., false))]
        );
    }
}
