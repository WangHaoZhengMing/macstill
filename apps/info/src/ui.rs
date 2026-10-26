use crate::{
    control::Server,
    model::{Category, Item, Report, Snapshot},
    request::Request,
};
use gpui_kit::{prelude::*, *};
use std::{collections::HashSet, time::Duration};
use swiftui_gpui::{Button, Fonts, Palette, button, caption, traffic_lights};

#[cfg(test)]
mod tests;

struct Data {
    snapshot: Snapshot,
    loading: bool,
    dark: bool,
}
#[derive(Default)]
struct Windows {
    about: Option<WindowHandle<About>>,
    report: Option<WindowHandle<SystemReport>>,
}
impl Global for Windows {}
struct Runtime {
    _poll: Task<()>,
}
impl Global for Runtime {}

fn load(data: Entity<Data>, cx: &mut App) {
    data.update(cx, |data, cx| {
        data.loading = true;
        cx.notify();
    });
    cx.spawn(async move |cx| {
        let snapshot = cx
            .background_executor()
            .spawn(async { crate::collect::collect(std::path::Path::new("/")) })
            .await;
        data.update(cx, |data, cx| {
            data.snapshot = snapshot;
            data.loading = false;
            cx.notify();
        });
    })
    .detach();
}

pub fn run(request: Request, server: Option<Server>) {
    gpui_kit::application().run(move |cx| {
        macstill_ui::init(cx);
        cx.set_global(Windows::default());
        let data = cx.new(|_| Data {
            snapshot: Snapshot::default(),
            loading: true,
            dark: macstill_config::load().map(|p| p.dark).unwrap_or(false),
        });
        load(data.clone(), cx);
        if let Err(error) = open(request, data.clone(), cx) {
            eprintln!("Cannot open System Information: {error:#}");
            cx.quit();
            return;
        }
        if let Some(server) = server {
            let poll = cx.spawn(async move |cx| {
                loop {
                    while let Some(request) = server.receive() {
                        cx.update(|cx| {
                            if let Err(error) = open(request, data.clone(), cx) {
                                eprintln!("Cannot open System Information: {error:#}");
                            }
                        });
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
            });
            cx.set_global(Runtime { _poll: poll });
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}

fn options(title: &str, dimensions: Size<Pixels>, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None, dimensions, cx,
        ))),
        titlebar: None,
        app_id: Some("org.macstill.Info".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        window_min_size: Some(if title == "About Macstill" {
            size(px(330.), px(580.))
        } else {
            size(px(640.), px(460.))
        }),
        ..Default::default()
    }
}

fn open(request: Request, data: Entity<Data>, cx: &mut App) -> anyhow::Result<()> {
    match request {
        Request::About => {
            if let Some(handle) = cx.global::<Windows>().about
                && handle
                    .update(cx, |_, w, cx| {
                        w.activate_window();
                        cx.notify();
                    })
                    .is_ok()
            {
                return Ok(());
            }
            let handle = cx.open_window(
                options("About Macstill", size(px(350.), px(620.)), cx),
                |w, cx| {
                    w.set_window_title("About Macstill");
                    cx.new(|cx| About::new(data, cx))
                },
            )?;
            cx.global_mut::<Windows>().about = Some(handle);
        }
        Request::Report => {
            if let Some(handle) = cx.global::<Windows>().report
                && handle.update(cx, |_, w, _| w.activate_window()).is_ok()
            {
                return Ok(());
            }
            let handle = cx.open_window(
                options("System Information", size(px(1000.), px(740.)), cx),
                |w, cx| {
                    w.set_window_title("System Information");
                    cx.new(|cx| SystemReport::new(data, cx))
                },
            )?;
            cx.global_mut::<Windows>().report = Some(handle);
        }
    }
    Ok(())
}

fn root(p: Palette, cx: &App) -> Div {
    div()
        .size_full()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(if p.dark { 0x505053 } else { 0xbcbcc0 }))
        .overflow_hidden()
        .bg(rgb(p.background))
        .text_color(rgb(p.text))
        .font_family(cx.global::<Fonts>().text.clone())
        .text_size(px(13.))
        .line_height(px(17.))
}

struct About {
    data: Entity<Data>,
    error: Option<String>,
    _subscription: Subscription,
}
impl About {
    fn new(data: Entity<Data>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&data, |_, _, cx| cx.notify());
        Self {
            data,
            error: None,
            _subscription: subscription,
        }
    }
}
impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.data.read(cx);
        let p = Palette::new(data.dark);
        swiftui_gpui::set_theme(p, cx);
        let data = self.data.read(cx);
        let snapshot = &data.snapshot;
        let mut specs = div().w_full().flex().flex_col().gap(px(6.));
        for (label, value) in [
            ("Processor", &snapshot.processor),
            ("Graphics", &snapshot.graphics),
            ("Memory", &snapshot.memory),
            ("Startup disk", &snapshot.startup_disk),
            ("Serial number", &snapshot.serial),
            ("System", &snapshot.operating_system),
        ] {
            specs = specs.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(99.))
                            .flex_shrink_0()
                            .text_right()
                            .text_color(rgb(p.muted))
                            .child(label),
                    )
                    .child(div().flex_1().min_w_0().child(value.clone())),
            );
        }
        let art = img(std::sync::Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../assets/computer.svg").to_vec(),
        )))
        .w(px(250.))
        .h(px(160.));
        root(p, cx)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("about-titlebar")
                    .h(px(38.))
                    .flex_shrink_0()
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .child(traffic_lights("about-window"))
                    .child(
                        div()
                            .id("about-drag")
                            .h_full()
                            .flex_1()
                            .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move()),
                    ),
            )
            .child(
                div()
                    .id("about-scroll")
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .px(px(18.))
                            .pt(px(17.))
                            .pb(px(25.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(17.))
                            .child(art)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .text_size(px(25.))
                                            .line_height(px(30.))
                                            .text_center()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(snapshot.model.clone()),
                                    )
                                    .child(
                                        caption(
                                            [snapshot.vendor.clone(), snapshot.version.clone()]
                                                .into_iter()
                                                .filter(|s| !s.is_empty())
                                                .collect::<Vec<_>>()
                                                .join(" · "),
                                            p,
                                        )
                                        .text_center(),
                                    ),
                            )
                            .child(specs)
                            .when(data.loading, |body| {
                                body.child(caption("Reading system information…", p))
                            })
                            .child(
                                button("more-info", "More Info…", p)
                                    .px(px(15.))
                                    .py(px(5.))
                                    .on_click(cx.listener(|s, _, _, cx| {
                                        if let Err(error) =
                                            open(Request::Report, s.data.clone(), cx)
                                        {
                                            s.error = Some(error.to_string());
                                            cx.notify();
                                        }
                                    })),
                            )
                            .when_some(self.error.clone(), |body, error| {
                                body.child(caption(error, p))
                            })
                            .child(
                                caption(
                                    format!(
                                        "Macstill {}\nSystem information",
                                        env!("CARGO_PKG_VERSION")
                                    ),
                                    p,
                                )
                                .text_size(px(11.))
                                .text_center(),
                            ),
                    ),
            )
    }
}

struct SystemReport {
    data: Entity<Data>,
    category: Category,
    selected: Option<String>,
    groups: [bool; 3],
    navigation: ScrollHandle,
    details: ScrollHandle,
    _subscription: Subscription,
}
impl SystemReport {
    fn new(data: Entity<Data>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&data, |_, _, cx| cx.notify());
        Self {
            data,
            category: Category::Hardware,
            selected: None,
            groups: [true; 3],
            navigation: ScrollHandle::new(),
            details: ScrollHandle::new(),
            _subscription: subscription,
        }
    }
    fn select_category(&mut self, category: Category, cx: &mut Context<Self>) {
        self.category = category;
        self.selected = None;
        self.details.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }
}

fn tree_rows(report: &Report) -> Vec<(usize, usize)> {
    fn add(
        items: &[Item],
        index: usize,
        depth: usize,
        seen: &mut HashSet<usize>,
        rows: &mut Vec<(usize, usize)>,
    ) {
        if !seen.insert(index) {
            return;
        }
        rows.push((index, depth));
        for (child, item) in items.iter().enumerate() {
            if item.parent.as_deref() == Some(items[index].id.as_str()) {
                add(items, child, depth + 1, seen, rows);
            }
        }
    }
    let mut rows = vec![];
    let mut seen = HashSet::new();
    for (index, item) in report.items.iter().enumerate() {
        if item
            .parent
            .as_ref()
            .is_none_or(|parent| !report.items.iter().any(|item| &item.id == parent))
        {
            add(&report.items, index, 0, &mut seen, &mut rows);
        }
    }
    for index in 0..report.items.len() {
        if !seen.contains(&index) {
            add(&report.items, index, 0, &mut seen, &mut rows);
        }
    }
    rows
}

impl Render for SystemReport {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = self.data.read(cx).dark;
        let p = Palette::new(dark);
        swiftui_gpui::set_theme(p, cx);
        let data = self.data.read(cx);
        let snapshot = data.snapshot.clone();
        let loading = data.loading;
        let report = snapshot.report(self.category);
        let selected = report.and_then(|report| {
            report
                .items
                .iter()
                .find(|item| self.selected.as_deref() == Some(&item.id))
                .or_else(|| report.items.first())
        });
        let mut navigation = div()
            .id("info-navigation")
            .track_scroll(&self.navigation)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.))
            .pb(px(12.));
        for (index, group) in ["Hardware", "Network", "Software"].into_iter().enumerate() {
            let expanded = self.groups[index];
            navigation = navigation.child(
                Button::new(("info-group", index))
                    .w_full()
                    .h(px(28.))
                    .flex()
                    .justify_start()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .text_color(rgb(p.text))
                    .child(if expanded { "⌄" } else { "›" })
                    .child(group)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.groups[index] = !s.groups[index];
                        cx.notify();
                    })),
            );
            if expanded {
                for category in Category::ALL
                    .into_iter()
                    .filter(|category| category.group() == group)
                {
                    navigation = navigation.child(
                        Button::new(format!("info-category-{}", category.title()))
                            .accessibility_label(category.title())
                            .w_full()
                            .h(px(27.))
                            .pl(px(29.))
                            .pr(px(7.))
                            .rounded(px(5.))
                            .flex()
                            .justify_start()
                            .text_size(px(13.))
                            .bg(if self.category == category {
                                rgb(if p.dark { 0x535357 } else { 0xc7c7ca }).into()
                            } else {
                                transparent_black()
                            })
                            .hover(move |s| s.bg(rgb(p.hover)))
                            .focus_visible(move |s| s.border_1().border_color(rgb(p.accent)))
                            .child(if category == Category::Hardware {
                                "Overview"
                            } else {
                                category.title()
                            })
                            .on_click(
                                cx.listener(move |s, _, _, cx| s.select_category(category, cx)),
                            ),
                    );
                }
            }
        }
        let sidebar = div()
            .w(px(205.))
            .flex_shrink_0()
            .h_full()
            .bg(rgb(if p.dark { 0x29292c } else { 0xdedee0 }))
            .border_r_1()
            .border_color(rgb(p.line))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("report-sidebar-titlebar")
                    .h(px(35.))
                    .px(px(14.))
                    .flex_shrink_0()
                    .child(traffic_lights("report-window")),
            )
            .child(navigation);
        let mut list = div()
            .id("info-items")
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .px(px(10.))
            .py(px(5.));
        if let Some(report) = report {
            for (position, (index, depth)) in tree_rows(report).into_iter().enumerate() {
                let item = &report.items[index];
                let id = item.id.clone();
                let active = selected.is_some_and(|selected| selected.id == item.id);
                let parent = report
                    .items
                    .iter()
                    .any(|child| child.parent.as_deref() == Some(&item.id));
                list = list.child(
                    Button::new(format!("info-item-{}", item.id))
                        .accessibility_label(item.name.clone())
                        .w_full()
                        .min_h(px(24.))
                        .py(px(4.))
                        .pr(px(9.))
                        .pl(px(6. + depth.min(12) as f32 * 17.))
                        .rounded(px(5.))
                        .flex()
                        .justify_start()
                        .gap(px(6.))
                        .bg(if active {
                            rgb(p.accent).into()
                        } else if position % 2 == 0 {
                            rgb(if p.dark { 0x2b2b2e } else { 0xf3f3f5 }).into()
                        } else {
                            transparent_black()
                        })
                        .text_color(rgb(if active { p.accent_text } else { p.text }))
                        .child(div().w(px(10.)).child(if parent { "⌄" } else { "" }))
                        .child(item.name.clone())
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.selected = Some(id.clone());
                            s.details.set_offset(point(px(0.), px(0.)));
                            cx.notify();
                        })),
                );
            }
        }
        let mut fields = div().p(px(18.)).flex().flex_col().gap(px(6.));
        if let Some(item) = selected {
            fields = fields.child(
                div()
                    .mb(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}:", item.name)),
            );
            for (key, value) in &item.fields {
                fields = fields.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(14.))
                        .child(div().w(px(165.)).flex_shrink_0().child(format!("{key}:")))
                        .child(div().flex_1().min_w_0().child(value.clone())),
                );
            }
            if item.fields.is_empty() {
                fields = fields.child(caption(
                    "No additional information is exposed for this device.",
                    p,
                ));
            }
        } else {
            fields = fields.child(caption(
                if loading {
                    "Reading system information…".into()
                } else {
                    report.and_then(|r| r.note.clone()).unwrap_or_else(|| {
                        format!(
                            "No {} information was reported by the system.",
                            self.category.title()
                        )
                    })
                },
                p,
            ));
        }
        let breadcrumb = format!(
            "▱  {}  ›  {}{}",
            snapshot.model,
            self.category.title(),
            selected
                .map(|item| format!("  ›  {}", item.name))
                .unwrap_or_default()
        );
        root(p, cx).flex().child(sidebar).child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(rgb(if p.dark { 0x202023 } else { 0xffffff }))
                .child(
                    div()
                        .id("report-titlebar")
                        .h(px(46.))
                        .flex_shrink_0()
                        .border_b_1()
                        .border_color(rgb(p.line))
                        .bg(rgb(p.background))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(
                            div()
                                .id("report-drag")
                                .flex_1()
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(snapshot.model.clone())
                                .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move()),
                        )
                        .child(
                            button("info-refresh", "↻", p)
                                .accessibility_label("Refresh system information")
                                .border_0()
                                .bg(transparent_black())
                                .p(px(2.))
                                .disabled(loading)
                                .on_click(cx.listener(|s, _, _, cx| load(s.data.clone(), cx))),
                        ),
                )
                .child(
                    div()
                        .h(px(210.))
                        .min_h(px(90.))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .px(px(18.))
                                .py(px(9.))
                                .border_b_1()
                                .border_color(rgb(p.line))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(if self.category == Category::Usb {
                                    "USB Device Tree"
                                } else {
                                    self.category.title()
                                }),
                        )
                        .child(list),
                )
                .child(
                    div()
                        .h(px(9.))
                        .flex_shrink_0()
                        .border_y_1()
                        .border_color(rgb(p.line))
                        .bg(rgb(p.background))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(4.)).rounded_full().bg(rgb(p.line))),
                )
                .child(
                    div()
                        .id("info-details")
                        .track_scroll(&self.details)
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .child(fields),
                )
                .child(
                    div()
                        .h(px(29.))
                        .flex_shrink_0()
                        .px(px(12.))
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .bg(rgb(p.background))
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .child(caption(breadcrumb, p)),
                ),
        )
    }
}
