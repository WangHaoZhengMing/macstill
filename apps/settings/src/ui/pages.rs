use super::*;
use swiftui_gpui::{
    settings_divider, settings_group, settings_navigation, settings_row, settings_sidebar,
};
impl SettingsView {
    pub(super) fn settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        use super::widgets::*;
        let snap = self.snapshot(cx);
        let p = settings_palette(snap.preferences.dark);
        let material = material::SettingsMaterial::for_window(p.dark, window.is_window_active());
        let radius =
            material::SettingsMaterial::radius(window.is_maximized() || window.is_fullscreen());
        let hairline = material::SettingsMaterial::hairline(window.scale_factor());
        let system = self.system_state(cx);
        if let Some(editor) = &mut self.editor
            && editor.pending
            && !system.busy
            && system.revision > editor.revision
        {
            if let Some(error) = &system.error {
                editor.error = Some(error.clone());
                editor.pending = false;
                for field in &editor.fields {
                    field
                        .input
                        .update(cx, |input, cx| input.set_readonly(false, cx));
                }
            } else {
                self.editor = None;
            }
        }
        let modal = if self.confirmation.is_some() {
            1
        } else if self.editor.is_some() {
            2
        } else if self.bluetooth_details.is_some() {
            3
        } else if self.connection_details.is_some() {
            4
        } else if self.form.is_some() {
            5
        } else {
            0
        };
        if modal != self.last_modal {
            if self.last_modal == 0 {
                self.previous_focus = window.focused(cx);
            }
            self.last_modal = modal;
            if modal == 0 {
                if let Some(focus) = self.previous_focus.take() {
                    focus.focus(window, cx);
                }
            } else if modal == 2 {
                if let Some(field) = self.editor.as_ref().and_then(|e| e.fields.first()) {
                    field.input.update(cx, |input, cx| input.focus(window, cx));
                }
            } else if modal == 5 {
                self.form_input
                    .update(cx, |input, cx| input.focus(window, cx));
            } else {
                self.sheet_focus.focus(window, cx);
            }
        }
        let query = self
            .navigation_search
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let account = std::env::var("USER").unwrap_or_else(|_| "Local Account".into());
        let initial = account
            .chars()
            .next()
            .unwrap_or('M')
            .to_uppercase()
            .to_string();
        let mut navigation = div()
            .id("settings-navigation")
            .track_scroll(&self.navigation_scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.));
        let mut last_group = None;
        let mut found = 0;
        let mut child_index = 0;
        for (i, page) in Page::ALL.into_iter().enumerate() {
            if query.is_empty()
                && matches!(
                    page,
                    Page::AirDrop
                        | Page::About
                        | Page::DateTime
                        | Page::LanguageRegion
                        | Page::Storage
                        | Page::LoginItems
                )
            {
                continue;
            }
            if !sidebar::matches_search(page, &query) {
                continue;
            }
            if last_group.is_some_and(|g| g != page.group()) {
                navigation = navigation.child(div().h(px(15.)).flex_shrink_0());
                child_index += 1;
            }
            last_group = Some(page.group());
            found += 1;
            let selected = self.page == page
                || (page == Page::General
                    && matches!(
                        self.page,
                        Page::AirDrop
                            | Page::About
                            | Page::DateTime
                            | Page::LanguageRegion
                            | Page::Storage
                            | Page::LoginItems
                    ));
            if selected && self.reveal_navigation > 0 {
                self.navigation_scroll.scroll_to_item(child_index);
                self.reveal_navigation -= 1;
                cx.notify();
            }
            child_index += 1;
            navigation = navigation.child(
                settings_navigation(
                    ("nav", i),
                    page.title(),
                    Self::navigation_icon(page, page_color(page)),
                    self.page == page
                        || (page == Page::General
                            && matches!(
                                self.page,
                                Page::AirDrop
                                    | Page::About
                                    | Page::DateTime
                                    | Page::LanguageRegion
                                    | Page::Storage
                                    | Page::LoginItems
                            )),
                    p,
                )
                .h(px(32.))
                .rounded(px(6.))
                .on_click(cx.listener(move |s, _, w, cx| s.set_page(page, w, cx))),
            );
        }
        if found == 0 {
            navigation = navigation.child(caption("No settings found", p).p(px(12.)));
        }
        let sidebar = settings_sidebar(p)
            .w(px(222.))
            .bg(material.sidebar)
            .border_r(hairline)
            .border_color(material.divider)
            .child(
                div()
                    .id("sidebar-drag")
                    .h(px(48.))
                    .flex_shrink_0()
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .child(traffic_lights("settings-window"))
                    .child(
                        div()
                            .id("sidebar-title-drag")
                            .flex_1()
                            .h_full()
                            .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move()),
                    ),
            )
            .child(
                div()
                    .relative()
                    .mb(px(12.))
                    .child(
                        text_field("navigation-search", &self.navigation_search, p, cx)
                            .h(px(30.))
                            .bg(material.search)
                            .border_color(rgba(0x00000008))
                            .pl(px(29.))
                            .pr(px(27.))
                            .rounded(px(6.)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(9.))
                            .top(px(7.))
                            .child(symbol("search", 14., p.muted)),
                    )
                    .when(!query.is_empty(), |d| {
                        d.child(
                            button("clear-settings-search", "×", p)
                                .absolute()
                                .right(px(3.))
                                .top(px(3.))
                                .w(px(24.))
                                .h(px(24.))
                                .p(px(0.))
                                .border_0()
                                .bg(transparent_black())
                                .accessibility_label("Clear search")
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.navigation_search
                                        .update(cx, |input, cx| input.set_value("", w, cx))
                                })),
                        )
                    }),
            )
            .child(
                button("account-about", "", p)
                    .border_0()
                    .bg(transparent_black())
                    .p(px(6.))
                    .mb(px(16.))
                    .w_full()
                    .flex()
                    .justify_start()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(42.))
                            .rounded_full()
                            .bg(rgb(if p.dark { 0x646974 } else { 0xa2a8b2 }))
                            .text_color(rgb(0xffffff))
                            .text_size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(initial),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .items_start()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(account),
                            )
                            .child(caption("Local Account", p).text_size(px(11.))),
                    )
                    .on_click(cx.listener(|s, _, w, cx| s.set_page(Page::About, w, cx))),
            )
            .child(navigation);
        let content = if self.picker.is_some() {
            self.picker_page(window, cx)
        } else {
            match self.page {
                Page::Wifi => self.wifi(cx),
                Page::Bluetooth => self.bluetooth(cx),
                Page::Network => self.network(cx),
                Page::General => self.general(cx),
                Page::AirDrop => self.airdrop_settings(cx),
                Page::Storage => self.storage(cx),
                Page::LoginItems => self.login_items(cx),
                Page::Sound => self.sound(window, cx),
                Page::Displays => self.displays(window, cx),
                Page::Battery => self.battery(cx),
                Page::DateTime => self.date_time(cx),
                Page::Shortcuts => self.shortcuts(cx),
                Page::Applications => self.applications(window, cx),
                Page::Appearance => self.appearance(cx),
                Page::ControlCenter => self.control_center_settings(cx),
                Page::Wallpaper => self.wallpaper_settings(cx),
                Page::Dock => self.dock_settings(window, cx),
                Page::About => self.system_about(cx),
                Page::LanguageRegion => self.language_region(cx),
                Page::Keyboard => self.keyboard(cx),
                Page::LockScreen => self.lock_screen(cx),
                Page::Mouse | Page::Trackpad => self.pointer_page(window, cx),
            }
        };
        let error = self.message.clone().or(snap.error).or(system.error.clone());
        let toolbar = div()
            .id("settings-titlebar")
            .bg(rgb(p.background))
            .h(px(54.))
            .flex_shrink_0()
            .px(px(18.))
            .flex()
            .gap(px(7.))
            .items_center()
            .child(
                button("navigation-back", "", p)
                    .child(symbol("chevron-left", 17., p.muted))
                    .accessibility_label("Back")
                    .border_0()
                    .bg(transparent_black())
                    .text_size(px(23.))
                    .p(px(0.))
                    .w(px(23.))
                    .h(px(30.))
                    .disabled(self.history_index == 0)
                    .on_click(cx.listener(|s, _, w, cx| s.history_move(false, w, cx))),
            )
            .child(
                button("navigation-forward", "", p)
                    .child(symbol("chevron-right", 17., p.muted))
                    .accessibility_label("Forward")
                    .border_0()
                    .bg(transparent_black())
                    .text_size(px(23.))
                    .p(px(0.))
                    .w(px(23.))
                    .h(px(30.))
                    .disabled(self.history_index + 1 == self.history.len())
                    .on_click(cx.listener(|s, _, w, cx| s.history_move(true, w, cx))),
            )
            .child(
                div()
                    .id("toolbar-title-drag")
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move())
                    .ml(px(7.))
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.page.title()),
            )
            .child(
                div()
                    .id("toolbar-space-drag")
                    .flex_1()
                    .h_full()
                    .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move()),
            )
            .when(system.busy || system.loading, |d| {
                d.child(caption(
                    if system.loading {
                        "Loading…"
                    } else {
                        "Applying…"
                    },
                    p,
                ))
            });
        let mut body = div()
            .relative()
            .tab_group()
            .size_full()
            .rounded(radius)
            .overflow_hidden()
            .font_family(cx.global::<Fonts>().text.clone())
            .text_size(px(13.))
            .text_color(rgb(p.text))
            // No root fill: only the sidebar reveals the real blurred scene.
            .flex()
            .capture_key_down(cx.listener(|s, event: &KeyDownEvent, window, cx| {
                if s.last_modal != 0 && event.keystroke.key == "tab" {
                    for _ in 0..256 {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        if s.sheet_focus.contains_focused(window, cx) {
                            break;
                        }
                    }
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|s, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    if s.editor.as_ref().is_some_and(|e| e.pending) {
                        return;
                    }
                    if s.wifi_details.as_ref().is_some_and(|draft| draft.pending) {
                        return;
                    }
                    if s.confirmation.is_some() {
                        s.confirmation = None;
                    } else if s.editor.is_some() {
                        s.editor = None;
                    } else if s.bluetooth_details.is_some() {
                        s.bluetooth_details = None;
                    } else if s.connection_details.is_some() {
                        s.connection_details = None;
                        s.store.read(cx).system.clear_connection();
                    } else if s.form.is_some() {
                        s.form = None;
                    } else if !s.navigation_search.read(cx).value().is_empty() {
                        s.navigation_search
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(sidebar)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .bg(rgb(material.content))
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(toolbar)
                    .child(
                        div()
                            .id("settings-scroll")
                            .track_scroll(&self.scroll)
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(20.))
                            .pb(px(24.))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(640.))
                                    .mx_auto()
                                    .pt(px(12.))
                                    .child(content),
                            ),
                    )
                    .when_some(error.clone().or(system.notice), |d, message| {
                        d.child(
                            div()
                                .px(px(16.))
                                .py(px(10.))
                                .border_t_1()
                                .border_color(rgb(p.line))
                                .bg(rgb(p.surface))
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(rgb(if error.is_some() {
                                            if p.dark { 0xffad91 } else { 0xa84025 }
                                        } else {
                                            p.muted
                                        }))
                                        .child(message),
                                )
                                .child(
                                    button("dismiss-message", "×", p)
                                        .border_0()
                                        .py(px(0.))
                                        .on_click(cx.listener(|s, _, _, cx| {
                                            s.message = None;
                                            s.store.read(cx).system.dismiss();
                                            cx.notify();
                                        })),
                                ),
                        )
                    }),
            );
        if self.form.is_some() {
            body = body.child(self.form_sheet(p, cx));
        }
        if self.connection_details.is_some() {
            body = body.child(self.connection_sheet(p, window, cx));
        }
        if self.bluetooth_details.is_some() {
            body = body.child(self.bluetooth_sheet(p, window, cx));
        }
        if self.editor.is_some() {
            body = body.child(self.editor_sheet(p, window, cx));
        }
        if self.confirmation.is_some() {
            body = body.child(self.confirmation_sheet(p, cx));
        }
        // No outside shadow gutter or uniform white outline. Hyprland owns
        // exterior shadow/rounding, with the same radius as the content clip.
        body.child(
            div()
                .absolute()
                .top_0()
                .left(radius)
                .right(radius)
                .h(hairline)
                .bg(material.highlight),
        )
        .into_any_element()
    }
    fn shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.snapshot(cx);
        let p = settings_palette(snap.preferences.dark);
        let mut rows = panel(p).flex().flex_col();
        for shortcut in &snap.preferences.shortcuts {
            let id = shortcut.id.clone();
            let toggle_id = id.clone();
            let bind_id = id.clone();
            let icon_id = id.clone();
            let reset_id = id.clone();
            let bound = shortcut
                .app_id
                .as_ref()
                .and_then(|id| snap.apps.iter().find(|a| &a.id == id));
            let detail = match (&shortcut.app_id, bound) {
                (_, Some(a)) => a.name.clone(),
                (Some(_), None) => "Application uninstalled · binding kept".into(),
                _ => "Not assigned · choose a Linux application".into(),
            };
            rows = rows.child(
                div()
                    .px(px(15.))
                    .py(px(13.))
                    .border_b_1()
                    .border_color(rgb(p.line))
                    .flex()
                    .gap(px(12.))
                    .items_center()
                    .child(macstill_ui::icon(
                        shortcut
                            .custom_icon
                            .clone()
                            .or_else(|| Some(Self::asset(&shortcut.asset))),
                        38.,
                    ))
                    .child(Self::row(shortcut.name.clone(), detail, p))
                    .child(
                        button(eid("bind", &id), "Choose app…", p).on_click(cx.listener(
                            move |s, _, _, cx| {
                                s.picker = Some(Picker::Bind(bind_id.clone()));
                                s.page_index = 0;
                                cx.notify()
                            },
                        )),
                    )
                    .child(button(eid("icon", &id), "Icon…", p).on_click(cx.listener(
                        move |s, _, _, cx| {
                            s.picker = Some(Picker::IconShortcut(icon_id.clone()));
                            cx.notify()
                        },
                    )))
                    .when(shortcut.custom_icon.is_some(), |d| {
                        d.child(button(eid("reset", &id), "Reset", p).on_click(cx.listener(
                            move |s, _, _, cx| {
                                s.edit(cx, |p| {
                                    if let Some(v) =
                                        p.shortcuts.iter_mut().find(|v| v.id == reset_id)
                                    {
                                        v.custom_icon = None
                                    }
                                })
                            },
                        )))
                    })
                    .child(
                        toggle(eid("visible", &id), shortcut.visible, p)
                            .accessibility_label(format!("Show {} in Dock", shortcut.name))
                            .on_change(cx.listener(move |s, checked: &bool, _, cx| {
                                s.edit(cx, |p| {
                                    if let Some(v) =
                                        p.shortcuts.iter_mut().find(|v| v.id == toggle_id)
                                    {
                                        v.visible = *checked
                                    }
                                })
                            })),
                    ),
            );
        }
        div().child(rows)
   .child(caption("Switch an entry off to hide it from the Dock. Its app and icon choices are kept, so it can return at any time.",p).mt(px(12.)))
   .into_any_element()
    }
    fn appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.snapshot(cx).preferences;
        let p = settings_palette(prefs.dark);
        let mut choices = div().flex().gap(px(12.));
        for (i, dark) in [false, true].into_iter().enumerate() {
            choices = choices.child(
                super::widgets::appearance_choice(("appearance", i), dark, p.dark == dark, p)
                    .on_click(cx.listener(move |s, _, _, cx| s.edit(cx, |p| p.dark = dark))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                settings_group(p).child(
                    settings_row("Appearance", p)
                        .items_start()
                        .py(px(14.))
                        .flex_wrap()
                        .child(choices),
                ),
            )
            .child(caption("Choose the appearance for your desktop and settings.", p).px(px(14.)))
            .child(settings_group(p).child(
                settings_row("Font", p).child(caption(cx.global::<Fonts>().text.clone(), p)),
            ))
            .into_any_element()
    }
    fn wallpaper_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.snapshot(cx).preferences;
        let p = settings_palette(prefs.dark);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                settings_group(p)
                    .child(
                        settings_row("Wallpaper", p)
                            .child(
                                button(
                                    "choose-wallpaper",
                                    if self.wallpaper_busy {
                                        "Importing…"
                                    } else {
                                        "Choose Image…"
                                    },
                                    p,
                                )
                                .disabled(self.wallpaper_busy)
                                .on_click(cx.listener(|s, _, _, cx| s.choose_wallpaper(cx))),
                            )
                            .child(
                                button("reset-wallpaper", "Use Default", p)
                                    .disabled(self.wallpaper_busy || prefs.wallpaper.is_none())
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        s.edit(cx, |p| p.wallpaper = None)
                                    })),
                            ),
                    )
                    .child(
                        div().px(px(14.)).pb(px(14.)).child(
                            img(macstill_ui::wallpaper(prefs.wallpaper.as_deref()))
                                .w_full()
                                .h(px(260.))
                                .object_fit(ObjectFit::Cover)
                                .rounded(px(7.)),
                        ),
                    ),
            )
            .child(
                caption(
                    "PNG, JPEG or WebP · Applied immediately to the desktop and Launchpad.",
                    p,
                )
                .px(px(14.)),
            )
            .into_any_element()
    }
    fn dock_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.snapshot(cx).preferences;
        let p = settings_palette(prefs.dark);
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Dock"),
            )
            .child(
                settings_group(p).child(
                    div()
                        .flex()
                        .items_stretch()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .p(px(12.))
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .text_size(px(13.))
                                .child("Size")
                                .child(self.slider(
                                    "dock-size".into(),
                                    prefs.dock_size as f64,
                                    super::widgets::SliderTarget::DockSize,
                                    window,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .child(caption("Small", p))
                                        .child(caption("Large", p)),
                                ),
                        )
                        .child(div().w(px(1.)).my(px(12.)).bg(rgb(p.line)))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .p(px(12.))
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .text_size(px(13.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(12.))
                                        .child("Magnification")
                                        .child(
                                            toggle(
                                                "dock-magnification",
                                                prefs.dock_magnification,
                                                p,
                                            )
                                            .accessibility_label("Magnification")
                                            .on_change(cx.listener(|s, checked: &bool, _, cx| {
                                                s.edit(cx, |p| p.dock_magnification = *checked)
                                            })),
                                        ),
                                )
                                .child(caption(
                                    "Enlarge icons when you move the pointer over them.",
                                    p,
                                )),
                        ),
                ),
            )
            .child(
                settings_group(p)
                    .child(
                        settings_row("Automatically hide and show the Dock", p).child(
                            toggle("dock-auto-hide", prefs.dock_auto_hide, p)
                                .accessibility_label("Automatically hide and show the Dock")
                                .on_change(cx.listener(|s, checked: &bool, _, cx| {
                                    s.edit(cx, |p| p.dock_auto_hide = *checked)
                                })),
                        ),
                    )
                    .child(settings_divider(p))
                    .child(
                        settings_row("Show running applications", p).child(
                            toggle("dock-running", prefs.dock_show_running, p)
                                .accessibility_label("Show running applications")
                                .on_change(cx.listener(|s, checked: &bool, _, cx| {
                                    s.edit(cx, |p| p.dock_show_running = *checked)
                                })),
                        ),
                    ),
            )
            .child(
                settings_group(p)
                    .child(
                        settings_row("App Shortcuts", p).child(
                            button("dock-shortcuts", "Configure…", p)
                                .py(px(4.))
                                .on_click(
                                    cx.listener(|s, _, w, cx| s.set_page(Page::Shortcuts, w, cx)),
                                ),
                        ),
                    )
                    .child(settings_divider(p))
                    .child(
                        settings_row("Applications", p).child(
                            button("dock-applications", "Manage…", p)
                                .py(px(4.))
                                .on_click(
                                    cx.listener(|s, _, w, cx| {
                                        s.set_page(Page::Applications, w, cx)
                                    }),
                                ),
                        ),
                    ),
            )
            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("Mission Control"))
            .child(settings_group(p)
                .child(settings_row("Group windows by application", p).child(
                    toggle("mission-group", prefs.mission_control.group_by_application, p)
                        .accessibility_label("Group windows by application")
                        .on_change(cx.listener(|s, checked: &bool, _, cx| {
                            s.edit(cx, |p| p.mission_control.group_by_application = *checked)
                        }))))
                .child(settings_divider(p))
                .child(settings_row("Animate Mission Control", p).child(
                    toggle("mission-animate", prefs.mission_control.animate, p)
                        .accessibility_label("Animate Mission Control")
                        .on_change(cx.listener(|s, checked: &bool, _, cx| {
                            s.edit(cx, |p| p.mission_control.animate = *checked)
                        }))))
                .child(settings_divider(p))
                .child(settings_row("Mission Control", p).child(caption("Control ↑  /  Super F3", p)))
                .child(settings_divider(p))
                .child(settings_row("Switch desktops", p).child(caption("Control ← / →", p)))
                .child(settings_divider(p))
                .child(div().p(px(12.)).child(caption(
                    "Swipe up with three fingers to open Mission Control, down to return, or sideways to switch desktops. Drag a window onto a desktop to move it. Desktop order stays fixed.", p))))
            .into_any_element()
    }
}
