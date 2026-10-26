use crate::Palette;
use gpui_kit::{base::Switch, prelude::*, *};

/// A controlled switch. The application owns its value and receives the next value.
#[derive(IntoElement)]
pub struct Toggle {
    base: Switch,
}

pub fn toggle(id: impl Into<ElementId>, checked: bool, p: Palette) -> Toggle {
    Toggle {
        base: Switch::new(id)
            .checked(checked)
            .w(px(34.))
            .h(px(20.))
            .flex_shrink_0()
            .rounded_full()
            .border_1()
            .border_color(rgba(0x00000000))
            .px(px(1.))
            .flex()
            .items_center()
            .when(checked, |d| d.justify_end())
            .bg(rgb(if checked { p.accent } else { p.switch_off }))
            .cursor_pointer()
            .focus_visible(move |s| s.border_color(rgb(p.text)))
            .styles(|s| s.disabled(|s| s.opacity(0.45).cursor_default()))
            .child(
                div()
                    .size(px(16.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(rgb(0xffffff))
                    .shadow_sm(),
            ),
    }
}

impl Toggle {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.base = self.base.disabled(disabled);
        self
    }
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.accessibility_label(label);
        self
    }
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.base = self.base.track_focus(focus);
        self
    }
    /// Compatible with `cx.listener(|view, checked: &bool, window, cx| ...)`.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.base = self
            .base
            .on_change(move |checked, _, w, cx| handler(&checked, w, cx));
        self
    }
}
impl Styled for Toggle {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}
impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.base
    }
}
