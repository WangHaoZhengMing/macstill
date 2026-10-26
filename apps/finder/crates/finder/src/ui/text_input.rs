//! Application events around GPUI Kit's input; editing, IME and selection belong to Kit.
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window, div,
    prelude::*,
};
use std::ops::Range;

pub struct Changed;
pub struct BackspaceAtStart;
pub struct Confirmed;
pub struct Cancelled;

pub struct TextInput {
    state: Entity<InputState>,
    content: SharedString,
    pending_text: Option<SharedString>,
    pending_selection: Option<Range<usize>>,
    _subscription: Subscription,
}
impl gpui_kit::EventEmitter<Changed> for TextInput {}
impl gpui_kit::EventEmitter<BackspaceAtStart> for TextInput {}
impl gpui_kit::EventEmitter<Confirmed> for TextInput {}
impl gpui_kit::EventEmitter<Cancelled> for TextInput {}

impl TextInput {
    pub fn new(
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.into();
        let state = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe(&state, |this, state, event, cx| match event {
            InputEvent::Change => {
                this.content = state.read(cx).value();
                cx.emit(Changed);
                cx.notify();
            }
            InputEvent::PressEnter { .. } => cx.emit(Confirmed),
            _ => {}
        });
        Self {
            state,
            content: SharedString::default(),
            pending_text: None,
            pending_selection: None,
            _subscription: subscription,
        }
    }
    pub fn text(&self) -> &str {
        &self.content
    }
    pub fn set_text(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = value.into();
        self.pending_text = Some(self.content.clone());
        cx.emit(Changed);
        cx.notify();
    }
    pub fn select_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.pending_selection = Some(range);
        cx.notify();
    }
}
impl Focusable for TextInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.focus_handle(cx)
    }
}
impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(value) = self.pending_text.take() {
            self.state
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        if let Some(range) = self.pending_selection.take() {
            self.state
                .update(cx, |state, cx| state.set_selected_range(range, cx));
        }
        div()
            .w_full()
            .capture_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(Cancelled);
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "backspace"
                    && this.state.read(cx).selected_range() == (0..0)
                {
                    cx.emit(BackspaceAtStart);
                }
            }))
            .child(Input::new(&self.state).appearance(false).bordered(false))
    }
}
