//! Procedural context menus based on macos-context-menu-foundation/FOUNDATION.md.
//! All metrics are application choices. No Apple assets or private filters are
//! embedded; without scene-backdrop sampling we explicitly use opaque material.
use super::theme;
use gpui_kit::{
    AnyElement, App, BoxShadow, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, Hsla,
    IntoElement, KeyDownEvent, MouseButton, Pixels, Point, Render, Role, ScrollHandle,
    SharedString, TextRun, Window, div, point, prelude::*, px, rgb, rgba, white,
};
use std::rc::Rc;

pub type Command = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub enum MenuRow {
    Command {
        label: SharedString,
        shortcut: Option<SharedString>,
        enabled: bool,
        checked: bool,
        dot: Option<Hsla>,
        run: Command,
    },
    Submenu {
        label: SharedString,
        rows: Vec<MenuRow>,
    },
    Header(SharedString),
    Separator,
}
impl MenuRow {
    fn selectable(&self) -> bool {
        matches!(
            self,
            Self::Command { enabled: true, .. } | Self::Submenu { .. }
        )
    }
    fn label(&self) -> SharedString {
        match self {
            Self::Command { label, .. } | Self::Submenu { label, .. } | Self::Header(label) => {
                label.clone()
            }
            Self::Separator => "".into(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct MenuMetrics {
    pub corner_radius: f32,
    pub outer_padding: f32,
    pub row_height: f32,
    pub selection_radius: f32,
    pub separator_height: f32,
    pub separator_inset: f32,
    pub column_gap: f32,
    pub shadow_margin: f32,
}
impl Default for MenuMetrics {
    fn default() -> Self {
        Self {
            corner_radius: 6.,
            outer_padding: 5.,
            row_height: 22.,
            selection_radius: 4.,
            separator_height: 9.,
            separator_inset: 10.,
            column_gap: 20.,
            shadow_margin: 40.,
        }
    }
}
impl MenuMetrics {
    fn height(&self, row: &MenuRow) -> f32 {
        if matches!(row, MenuRow::Separator) {
            self.separator_height
        } else {
            self.row_height
        }
    }
}

struct MenuMaterial {
    fill: Hsla,
    foreground: Hsla,
    secondary: Hsla,
    disabled: Hsla,
    separator: Hsla,
    selection: Hsla,
    border: Hsla,
    shadow: Vec<BoxShadow>,
}
impl MenuMaterial {
    fn current() -> Self {
        let dark = theme::dark();
        Self {
            fill: rgb(if dark { 0x282828 } else { 0xF6F6F6 }).into(),
            foreground: theme::text().into(),
            secondary: theme::text_secondary().into(),
            disabled: rgba(if dark { 0xFFFFFF50 } else { 0x00000050 }).into(),
            separator: rgba(if dark { 0xFFFFFF26 } else { 0x00000024 }).into(),
            selection: theme::selection().into(),
            border: rgba(if dark { 0xFFFFFF2A } else { 0x00000026 }).into(),
            shadow: vec![
                BoxShadow {
                    color: rgba(0x00000018).into(),
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: false,
                },
                BoxShadow {
                    color: rgba(if dark { 0x00000060 } else { 0x00000030 }).into(),
                    offset: point(px(0.), px(8.)),
                    blur_radius: px(10.),
                    spread_radius: px(0.),
                    inset: false,
                },
            ],
        }
    }
}

#[derive(Default)]
struct MenuState {
    open_path: Vec<usize>,
    highlighted: Vec<Option<usize>>,
}
fn next_selectable(rows: &[MenuRow], current: Option<usize>, forward: bool) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    let start = current.unwrap_or(if forward { rows.len() - 1 } else { 0 });
    (1..=rows.len())
        .map(|n| {
            if forward {
                (start + n) % rows.len()
            } else {
                (start + rows.len() - n) % rows.len()
            }
        })
        .find(|&i| rows[i].selectable())
}

/// A single focus owner for the complete menu tree. Pointer travel between
/// adjacent panels does not dismiss a submenu; only entering another row does.
pub struct ContextMenu {
    rows: Vec<MenuRow>,
    at: Point<Pixels>,
    font: SharedString,
    focus: FocusHandle,
    metrics: MenuMetrics,
    state: MenuState,
    scrolls: Vec<ScrollHandle>,
}
impl EventEmitter<DismissEvent> for ContextMenu {}
impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl ContextMenu {
    pub fn new(
        rows: Vec<MenuRow>,
        at: Point<Pixels>,
        font: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            rows,
            at,
            font,
            focus: cx.focus_handle(),
            metrics: MenuMetrics::default(),
            state: MenuState::default(),
            scrolls: vec![ScrollHandle::new()],
        }
    }
    fn rows_at(&self, depth: usize) -> &[MenuRow] {
        let mut rows = self.rows.as_slice();
        for &ix in self.state.open_path.iter().take(depth) {
            let Some(MenuRow::Submenu { rows: children, .. }) = rows.get(ix) else {
                break;
            };
            rows = children;
        }
        rows
    }
    fn highlight(&mut self, depth: usize, ix: usize, open: bool, cx: &mut Context<Self>) {
        self.state.open_path.truncate(depth);
        self.state.highlighted.resize(depth + 1, None);
        self.state.highlighted.truncate(depth + 1);
        self.state.highlighted[depth] = Some(ix);
        if open && matches!(self.rows_at(depth).get(ix), Some(MenuRow::Submenu { .. })) {
            self.state.open_path.push(ix);
            self.state.highlighted.push(None);
            if self.scrolls.len() <= depth + 1 {
                self.scrolls.push(ScrollHandle::new());
            }
        }
        cx.notify();
    }
    fn activate(&mut self, depth: usize, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        match self.rows_at(depth).get(ix).cloned() {
            Some(MenuRow::Command {
                enabled: true, run, ..
            }) => {
                cx.emit(DismissEvent);
                run(window, cx);
            }
            Some(MenuRow::Submenu { .. }) => {
                self.highlight(depth, ix, true, cx);
                self.state.highlighted[depth + 1] =
                    next_selectable(self.rows_at(depth + 1), None, true);
                if let Some(ix) = self.state.highlighted[depth + 1] {
                    self.scrolls[depth + 1].scroll_to_item(ix);
                }
            }
            _ => {}
        }
    }
    fn key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let depth = self.state.open_path.len();
        let current = self.state.highlighted.get(depth).copied().flatten();
        match key {
            "escape" => cx.emit(DismissEvent),
            "up" | "down" | "home" | "end" => {
                let start = if matches!(key, "home" | "end") {
                    None
                } else {
                    current
                };
                if let Some(ix) =
                    next_selectable(self.rows_at(depth), start, matches!(key, "down" | "home"))
                {
                    self.highlight(depth, ix, false, cx);
                    self.scrolls[depth].scroll_to_item(ix);
                }
            }
            "enter" | "space" | "right" => {
                if let Some(ix) = current {
                    if key != "right" || matches!(self.rows_at(depth)[ix], MenuRow::Submenu { .. })
                    {
                        self.activate(depth, ix, window, cx);
                    }
                }
            }
            "left" if depth > 0 => {
                self.state.open_path.pop();
                self.state.highlighted.truncate(depth);
                cx.notify();
            }
            _ => {}
        }
    }
    fn text_width(&self, text: SharedString, window: &Window) -> f32 {
        let mut font = window.text_style().font();
        font.family = self.font.clone();
        let run = TextRun {
            len: text.len(),
            font,
            color: white(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text, px(13.), &[run], None)
            .width
            / px(1.)
    }
    fn width(&self, rows: &[MenuRow], window: &Window) -> (f32, f32) {
        let label = rows
            .iter()
            .map(|row| self.text_width(row.label(), window))
            .fold(0., f32::max);
        let shortcut = rows
            .iter()
            .filter_map(|row| match row {
                MenuRow::Command { shortcut, .. } => shortcut.clone(),
                _ => None,
            })
            .map(|s| self.text_width(s, window))
            .fold(0., f32::max);
        // Shared checkmark, label, shortcut and arrow columns for this panel.
        (
            (12. + 18. + label.ceil() + self.metrics.column_gap + shortcut.ceil() + 18. + 12.)
                .max(200.),
            shortcut.ceil() + 2.,
        )
    }
    fn panel(
        &self,
        rows: &[MenuRow],
        depth: usize,
        at: Point<Pixels>,
        width: f32,
        shortcut_width: f32,
        height: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let m = self.metrics;
        let material = MenuMaterial::current();
        let selected = self.state.highlighted.get(depth).copied().flatten();
        let scale = window.scale_factor();
        let mut body = div()
            .id(("context-menu-items", depth))
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scrolls[depth]);
        for (ix, row) in rows.iter().enumerate() {
            if matches!(row, MenuRow::Separator) {
                body = body.child(
                    div()
                        .h(px(m.separator_height))
                        .flex_none()
                        .flex()
                        .items_center()
                        .px(px(m.separator_inset))
                        .child({
                            let color = material.separator;
                            gpui_kit::canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    let line = gpui_kit::Bounds::new(
                                        point(
                                            window.pixel_snap(bounds.left()),
                                            window.pixel_snap(bounds.center().y),
                                        ),
                                        gpui_kit::size(
                                            bounds.size.width,
                                            px(1. / window.scale_factor()),
                                        ),
                                    );
                                    window.paint_quad(gpui_kit::fill(line, color));
                                },
                            )
                            .w_full()
                            .h_full()
                        }),
                );
                continue;
            }
            let enabled = row.selectable();
            let highlighted = enabled && selected == Some(ix);
            let color = if highlighted {
                white()
            } else if matches!(row, MenuRow::Header(_)) {
                material.secondary
            } else if enabled {
                material.foreground
            } else {
                material.disabled
            };
            let (checked, dot, shortcut, submenu) = match row {
                MenuRow::Command {
                    checked,
                    dot,
                    shortcut,
                    ..
                } => (*checked, *dot, shortcut.clone(), false),
                MenuRow::Submenu { .. } => (false, None, None, true),
                _ => (false, None, None, false),
            };
            body = body.child(
                div()
                    .id(("context-menu-row", ix))
                    .role(if matches!(row, MenuRow::Header(_)) {
                        Role::Label
                    } else {
                        Role::MenuItem
                    })
                    .aria_label(row.label())
                    .aria_selected(highlighted)
                    .when(checked, |row| {
                        row.role(Role::MenuItemCheckBox)
                            .aria_toggled(gpui_kit::Toggled::True)
                    })
                    .when(highlighted, |row| row.aria_active_descendant())
                    .when(submenu, |row| {
                        row.aria_expanded(self.state.open_path.get(depth) == Some(&ix))
                    })
                    .when(!enabled, |row| row.aria_description("Unavailable"))
                    .when_some(shortcut.clone(), |row, shortcut| {
                        row.aria_keyshortcuts(shortcut)
                    })
                    .h(px(m.row_height))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(5.))
                    .rounded(px(m.selection_radius))
                    .text_color(color)
                    .when(highlighted, |row| row.bg(material.selection))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if !*hovered {
                            return;
                        }
                        if !enabled {
                            this.state.open_path.truncate(depth);
                            this.state.highlighted.resize(depth + 1, None);
                            this.state.highlighted.truncate(depth + 1);
                            this.state.highlighted[depth] = None;
                            cx.notify();
                        } else if this.state.highlighted.get(depth).copied().flatten() != Some(ix)
                            || (submenu && this.state.open_path.get(depth) != Some(&ix))
                        {
                            this.highlight(depth, ix, true, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.activate(depth, ix, window, cx);
                    }))
                    .child(
                        div()
                            .w(px(18.))
                            .flex_none()
                            .child(if checked { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .when_some(dot, |row, color| {
                                row.child(div().size(px(10.)).flex_none().rounded_full().bg(color))
                            })
                            .child(row.label()),
                    )
                    .child(
                        div()
                            .w(px(shortcut_width))
                            .text_right()
                            .ml(px(m.column_gap))
                            .flex_none()
                            .text_color(if highlighted {
                                white()
                            } else if enabled {
                                material.secondary
                            } else {
                                material.disabled
                            })
                            .child(shortcut.unwrap_or_default()),
                    )
                    .child(div().w(px(13.)).ml(px(5.)).flex_none().child(if submenu {
                        "›"
                    } else {
                        ""
                    })),
            );
        }
        div()
            .id(("context-menu-panel", depth))
            .role(Role::Menu)
            .absolute()
            .left(at.x)
            .top(at.y)
            .w(px(width))
            .h(px(height))
            .p(px(m.outer_padding))
            .flex()
            .flex_col()
            .rounded(px(m.corner_radius))
            .bg(material.fill)
            .shadow(material.shadow)
            .border(px(1. / scale))
            .border_color(material.border)
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(body)
            .into_any_element()
    }
}
impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let m = self.metrics;
        let margin = m.shadow_margin.min((viewport.height / px(1.)) / 4.);
        let mut at = self.at;
        let mut root = div()
            .id("finder-context-menu")
            .absolute()
            .size_full()
            .occlude()
            .key_context("FinderContextMenu")
            .track_focus(&self.focus)
            .font_family(self.font.clone())
            .text_size(px(13.))
            .line_height(px(18.))
            .whitespace_nowrap()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                this.key(&event.keystroke.key, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(DismissEvent);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(DismissEvent);
                }),
            );
        for depth in 0..=self.state.open_path.len() {
            let rows = self.rows_at(depth);
            let (width, shortcut) = self.width(rows, window);
            let width = width.min(viewport.width / px(1.) - margin * 2.);
            let height = (rows.iter().map(|row| m.height(row)).sum::<f32>() + m.outer_padding * 2.)
                .min(viewport.height / px(1.) - margin * 2.);
            at.x =
                at.x.max(px(margin))
                    .min(viewport.width - px(margin + width));
            at.y =
                at.y.max(px(margin))
                    .min(viewport.height - px(margin + height));
            root = root.child(self.panel(rows, depth, at, width, shortcut, height, window, cx));
            if let Some(&ix) = self.state.open_path.get(depth) {
                let row_y = rows.iter().take(ix).map(|r| m.height(r)).sum::<f32>();
                let next_width = self.width(self.rows_at(depth + 1), window).0;
                let right = at.x + px(width - 2.);
                at.x = if right + px(next_width + margin) > viewport.width {
                    at.x - px(next_width - 2.)
                } else {
                    right
                };
                at.y += px(row_y) + self.scrolls[depth].offset().y;
            }
        }
        root
    }
}

#[cfg(test)]
#[path = "../tests/menu.rs"]
mod tests;
