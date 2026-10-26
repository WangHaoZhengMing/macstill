//! Sidebar places, external volumes and their lifecycle.
use super::*;
use gpui_kit::component::InteractiveElementExt;

impl Browser {
    pub(super) fn start_drive_monitor(&mut self, cx: &mut Context<Self>) {
        if !cfg!(test) {
            self.sidebar.monitor = Some(cx.spawn(async |this, cx| {
                loop {
                    let result = cx
                        .background_spawn(async { crate::services::drives::discover() })
                        .await;
                    if this
                        .update(cx, |browser, cx| match result {
                            Ok(drives) => {
                                if browser.sidebar.drives != drives
                                    || browser.sidebar.error.is_some()
                                {
                                    let removed = crate::services::drives::removed_mounts(
                                        &browser.sidebar.drives,
                                        &drives,
                                    );
                                    if removed.iter().any(|path| {
                                        browser
                                            .navigation
                                            .location
                                            .directory()
                                            .is_some_and(|directory| directory.starts_with(path))
                                    }) {
                                        browser.navigate(home(), cx);
                                        browser.navigation.back.retain(|entry| {
                                            !removed.iter().any(|path| {
                                                entry.directory().is_some_and(|directory| {
                                                    directory.starts_with(path)
                                                })
                                            })
                                        });
                                        browser.navigation.forward.retain(|entry| {
                                            !removed.iter().any(|path| {
                                                entry.directory().is_some_and(|directory| {
                                                    directory.starts_with(path)
                                                })
                                            })
                                        });
                                        browser.commands.notice =
                                            Some("External drive disconnected or unmounted".into());
                                    }
                                    browser.sidebar.drives = drives;
                                    browser.sidebar.error = None;
                                    cx.notify();
                                }
                            }
                            Err(error) => {
                                if browser.sidebar.error.as_ref() != Some(&error) {
                                    browser.sidebar.error = Some(error);
                                    cx.notify();
                                }
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                    cx.background_executor().timer(Duration::from_secs(3)).await;
                }
            }));
        }
    }
    pub(super) fn open_drive(
        &mut self,
        drive: crate::services::drives::Drive,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = drive.mountpoint {
            self.navigate(path, cx);
            return;
        }
        if self.sidebar.mounting.is_some() {
            return;
        }
        self.sidebar.mounting = Some(drive.device.clone());
        self.commands.notice = Some(format!("Mounting {}…", drive.label));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { crate::services::drives::mount(&drive) })
                .await;
            let _ = this.update(cx, |browser, cx| {
                browser.sidebar.mounting = None;
                match result {
                    Ok(path) => {
                        browser.commands.notice = None;
                        browser.navigate(path, cx);
                    }
                    Err(error) => browser.commands.notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn traffic_lights(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = window.is_window_active();
        let light = |id: &'static str,
                     color: gpui_kit::Rgba,
                     glyph: &'static str,
                     area: WindowControlArea| {
            div()
                .id(id)
                .size(px(t::LIGHT_D))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(if active { color } else { t::light_inactive() })
                .border(px(0.5))
                .border_color(t::light_border())
                .on_mouse_down(MouseButton::Left, |_, w, cx| {
                    w.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |this, _, w, cx| match area {
                    WindowControlArea::Close => w.remove_window(),
                    WindowControlArea::Min => this.minimize(w, cx),
                    WindowControlArea::Max => w.zoom_window(),
                    _ => {}
                }))
                .child(
                    svg()
                        .path(symbol(glyph))
                        .size(px(t::LIGHT_D * 0.8))
                        .text_color(gpui_kit::transparent_black())
                        .group_hover("lights", |s| s.text_color(t::light_glyph())),
                )
        };
        div()
            .group("lights")
            .absolute()
            .top(px(t::LIGHTS_Y))
            .left(px(t::LIGHTS_X))
            .flex()
            .gap(px(t::LIGHT_GAP))
            .child(light(
                "close",
                t::light_close(),
                "xmark@semibold",
                WindowControlArea::Close,
            ))
            .child(light(
                "min",
                t::light_min(),
                "minus@semibold",
                WindowControlArea::Min,
            ))
            .child(light(
                "max",
                t::light_max(),
                "plus@semibold",
                WindowControlArea::Max,
            ))
    }
    pub(super) fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::{
            Icon, Selectable,
            button::{Button, ButtonVariants},
        };
        let mut content = div()
            .id("places")
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .pb_3()
            .px_2();
        for (section_ix, section) in self.sidebar.sections.iter().enumerate() {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(t::text_secondary())
                    .mt_3()
                    .mb_1()
                    .child(section.title),
            );
            for (place_ix, place) in section.places.iter().enumerate() {
                let active = place
                    .location
                    .as_ref()
                    .is_some_and(|target| self.navigation.location.sidebar_matches(target));
                let location = place.location.clone();
                let item = Button::new(("place-button", section_ix * 100 + place_ix))
                    .ghost()
                    .w_full()
                    .h(px(30.))
                    .justify_start()
                    .text_size(px(t::TEXT_SIZE))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(Icon::empty().path(symbol(place.icon)).size(px(18.)))
                            .child(place.name.clone()),
                    )
                    .selected(active)
                    .when(active, |button| button.text_color(t::accent()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(location) = &location {
                            this.navigate(location.clone(), cx);
                        }
                    }));
                content = content.child(
                    div()
                        .id(("place", section_ix * 100 + place_ix))
                        .when_some(
                            place
                                .location
                                .as_ref()
                                .and_then(Location::directory)
                                .map(Path::to_path_buf),
                            |element, destination| dnd::folder_target(element, destination, cx),
                        )
                        .when(place.location == Some(Location::AirDrop), |element| {
                            element
                                .on_drop(cx.listener(|this, drop: &dnd::Dragged, _, cx| {
                                    cx.stop_propagation();
                                    this.airdrop.files = drop.paths.clone();
                                    this.navigate(Location::AirDrop, cx);
                                }))
                                .on_drop(cx.listener(
                                    |this, drop: &gpui_kit::ExternalPaths, _, cx| {
                                        cx.stop_propagation();
                                        this.airdrop.files = drop.paths().to_vec();
                                        this.navigate(Location::AirDrop, cx);
                                    },
                                ))
                        })
                        .child(item),
                );
            }
        }
        content = content.child(
            div()
                .text_xs()
                .text_color(t::text_secondary())
                .mt_3()
                .mb_1()
                .child("External Drives"),
        );
        for drive in &self.sidebar.drives {
            let active = drive.mountpoint.as_deref().is_some_and(|path| {
                self.navigation
                    .location
                    .directory()
                    .is_some_and(|directory| directory.starts_with(path))
            });
            let drive = drive.clone();
            let label = if self.sidebar.mounting.as_ref() == Some(&drive.device) {
                format!("{} (mounting…)", drive.label)
            } else if drive.mountpoint.is_none() {
                format!(
                    "{} ({})",
                    drive.label,
                    if drive.locked {
                        "locked"
                    } else {
                        "not mounted"
                    }
                )
            } else {
                drive.label.clone()
            };
            let target = drive.mountpoint.clone();
            content = content.child(
                div()
                    .id(gpui_kit::SharedString::from(format!(
                        "drive-target-{}",
                        drive.device.display()
                    )))
                    .when_some(target, |element, path| {
                        dnd::folder_target(element, path, cx)
                    })
                    .child(
                        Button::new(gpui_kit::SharedString::from(format!(
                            "drive-{}",
                            drive.device.display()
                        )))
                        .ghost()
                        .w_full()
                        .h(px(30.))
                        .selected(active)
                        .when(active, |button| button.text_color(t::accent()))
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(Icon::empty().path(symbol("externaldrive")).size(px(18.)))
                                .child(div().min_w_0().truncate().child(label)),
                        )
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.open_drive(drive.clone(), cx)),
                        ),
                    ),
            );
        }
        if self.sidebar.drives.is_empty() {
            content = content.child(
                div()
                    .px_2()
                    .text_size(px(t::SMALL_SIZE))
                    .text_color(t::text_tertiary())
                    .child(if self.sidebar.error.is_some() {
                        "Drive detection unavailable"
                    } else {
                        "No external drives"
                    }),
            );
        }
        content = content.child(
            div()
                .text_xs()
                .text_color(t::text_secondary())
                .mt_3()
                .mb_1()
                .child("Tags"),
        );
        let showing = self.search.session.as_ref().and_then(|search| search.tag);
        content = content.child(div().flex().flex_col().gap(px(2.)).children(
            Tag::ALL.into_iter().enumerate().map(|(index, tag)| {
                Button::new(("tag-button", index))
                    .ghost()
                    .w_full()
                    .h(px(30.))
                    .justify_start()
                    .selected(showing == Some(tag))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(div().size(px(10.)).rounded_full().bg(tag.color()))
                            .child(tag.name()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.show_tag(tag, cx)))
            }),
        ));

        div()
            .relative()
            .w(px(t::SIDEBAR_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(t::sidebar_material(window.is_window_active()))
            .rounded_l(crate::ui::window::radius(window))
            .border_r_1()
            .border_color(t::separator())
            .child(
                div()
                    .h(px(t::TOOLBAR_H))
                    .flex_none()
                    .child(crate::ui::window::drag_area(
                        "sidebar-drag",
                        div().size_full(),
                    )),
            )
            .child(crate::ui::scrolling::vertical(
                "sidebar",
                &self.sidebar.scroll,
                div()
                    .id("sidebar-viewport")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .lock_scroll_axis()
                    .track_scroll(&self.sidebar.scroll)
                    .child(content),
            ))
            .child(self.traffic_lights(window, cx))
    }
}

pub(super) fn sections() -> Vec<Section> {
    let h = home();
    let known = |name: &str, icon, sub: &str| {
        let path = crate::services::xdg::user_dir(sub);
        (path != h && path.is_dir()).then(|| Place {
            name: name.into(),
            icon,
            location: Some(Location::Folder(path)),
        })
    };
    vec![
        Section {
            title: "Favorites",
            places: [
                Some(Place {
                    name: "Applications".into(),
                    icon: "applications",
                    location: Some(Location::Applications),
                }),
                known("Desktop", "menubar.dock.rectangle", "Desktop"),
                known("Documents", "document", "Documents"),
                Some(Place {
                    name: "AirDrop".into(),
                    icon: "airdrop",
                    location: Some(Location::AirDrop),
                }),
                known("Downloads", "arrow.down.circle", "Downloads"),
                known("Developer", "hammer", "Code"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
        Section {
            title: "Locations",
            places: [
                known("OneDrive", "icloud", "OneDrive"),
                Some(Place {
                    name: h
                        .file_name()
                        .map_or("Home".into(), |name| name.to_string_lossy().into_owned())
                        .into(),
                    icon: "house",
                    location: Some(Location::Folder(h)),
                }),
                Some(Place {
                    name: "File System".into(),
                    icon: "internaldrive",
                    location: Some(Location::Folder("/".into())),
                }),
                Some(Place {
                    name: "Network".into(),
                    icon: "network",
                    location: None,
                }),
                Some(Place {
                    name: "Trash".into(),
                    icon: "trash",
                    location: Some(Location::Trash),
                }),
            ]
            .into_iter()
            .flatten()
            .collect(),
        },
    ]
}

impl Browser {
    fn minimize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
            window.minimize_window();
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async {
                    let value: serde_json::Value =
                        serde_json::from_str(&macstill_hyprland::request("j/activewindow")?)?;
                    anyhow::ensure!(
                        value["pid"].as_u64() == Some(std::process::id() as u64),
                        "Finder is no longer the active window"
                    );
                    let address = value["address"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Window unavailable"))?;
                    let class = value["class"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Window unavailable"))?;
                    macstill_hyprland::window_menu::perform(
                        address,
                        class,
                        macstill_hyprland::window_menu::Action::Minimize,
                    )
                })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |browser, cx| {
                    browser.commands.notice = Some(error.to_string());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
