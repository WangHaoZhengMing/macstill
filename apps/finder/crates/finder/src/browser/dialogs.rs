//! Operation progress and conflict choices shared by paste, duplicate and drag/drop.
use super::*;
use crate::services::jobs::Decision;
use gpui_kit::component::button::{Button, ButtonVariants};
impl Browser {
    pub(super) fn render_job(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let job = self.commands.job.as_ref()?;
        let p = job.progress.lock().unwrap().clone();
        let mut panel = div()
            .absolute()
            .bottom(px(52.))
            .left(px(t::SIDEBAR_W + 24.))
            .w(px(480.))
            .p_4()
            .rounded_lg()
            .bg(t::content_bg())
            .border_1()
            .border_color(t::separator())
            .flex()
            .flex_col()
            .gap_2()
            .child(format!(
                "{} completed · {} transferred",
                p.completed,
                files::format_bytes(p.bytes)
            ))
            .child(div().truncate().child(p.path.display().to_string()));
        if let Some(c) = p.conflict {
            panel = panel
                .child(format!(
                    "“{}” already exists.",
                    files::display_name(&c.destination)
                ))
                .child(
                    div()
                        .truncate()
                        .child(format!("From: {}", c.source.display())),
                )
                .child(
                    div()
                        .truncate()
                        .child(format!("To: {}", c.destination.display())),
                )
                .child(
                    Button::new("conflict-all")
                        .ghost()
                        .label(if self.commands.apply_all {
                            "✓ Apply to all conflicts"
                        } else {
                            "Apply to all conflicts"
                        })
                        .on_click(cx.listener(|b, _, _, cx| {
                            b.commands.apply_all = !b.commands.apply_all;
                            cx.notify();
                        })),
                );
            let mut buttons = div().flex().flex_wrap().gap_2();
            for (id, label, decision) in [
                ("skip", "Skip", Decision::Skip),
                ("keep-both", "Keep Both", Decision::KeepBoth),
                ("replace", "Replace", Decision::Replace),
                ("merge", "Merge", Decision::Merge),
            ] {
                if decision == Decision::Merge && !c.directories {
                    continue;
                }
                buttons = buttons.child(Button::new(id).label(label).on_click(cx.listener(
                    move |b, _, _, cx| {
                        if let Some(job) = &b.commands.job {
                            job.answer(decision, b.commands.apply_all);
                        }
                        cx.notify();
                    },
                )));
            }
            panel = panel.child(buttons);
        }
        panel = panel.child(
            Button::new("cancel-operation")
                .label("Cancel")
                .on_click(cx.listener(|b, _, _, cx| {
                    if let Some(job) = &b.commands.job {
                        job.cancel();
                    }
                    cx.notify();
                })),
        );
        Some(
            gpui_kit::deferred(panel)
                .with_priority(4)
                .into_any_element(),
        )
    }
}

pub(super) fn folder_path(text: &str, base: &Path) -> Result<PathBuf, String> {
    if text.is_empty() {
        return Err("Enter a folder path.".into());
    }
    let path = if text == "~" {
        home()
    } else if let Some(rest) = text.strip_prefix("~/") {
        home().join(rest)
    } else if text.starts_with("file:") {
        url::Url::parse(text)
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .ok_or("Enter a local folder URL.")?
    } else {
        PathBuf::from(text)
    };
    Ok(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}
impl Browser {
    pub(super) fn toggle_hidden(
        &mut self,
        _: &ToggleHidden,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.listing.show_hidden = !self.listing.show_hidden;
        self.view.columns.clear();
        self.refresh(&[], cx);
    }
    pub(super) fn go_to_folder(
        &mut self,
        _: &GoToFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.commands.renaming.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            let mut input = TextInput::new("Folder path", window, cx);
            let path = self.navigation.directory.display().to_string();
            input.set_text(path.clone(), cx);
            input.select_range(0..path.len(), cx);
            input
        });
        let subs = vec![
            cx.subscribe_in(&input, window, |b, _, _: &text_input::Confirmed, w, cx| {
                b.confirm_go_to(w, cx)
            }),
            cx.subscribe_in(&input, window, |b, _, _: &text_input::Cancelled, w, cx| {
                b.commands.go_to = None;
                w.focus(&b.focus, cx);
                cx.notify();
            }),
        ];
        window.focus(&input.focus_handle(cx), cx);
        self.commands.go_to = Some(GoToDialog {
            input,
            _subs: subs,
            error: None,
        });
        cx.notify();
    }
    fn confirm_go_to(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.commands.go_to else {
            return;
        };
        match folder_path(dialog.input.read(cx).text(), &self.navigation.directory) {
            Ok(path) => {
                self.commands.go_to = None;
                w.focus(&self.focus, cx);
                self.navigate(path, cx);
            }
            Err(e) => {
                dialog.error = Some(e);
                cx.notify();
            }
        }
    }
    pub(super) fn render_go_to(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.commands.go_to.as_ref()?;
        let panel = div()
            .absolute()
            .top(px(76.))
            .left(px(230.))
            .w(px(480.))
            .p_4()
            .bg(t::content_bg())
            .rounded_lg()
            .border_1()
            .border_color(t::separator())
            .flex()
            .flex_col()
            .gap_3()
            .child("Go to Folder")
            .child(
                div()
                    .border_1()
                    .border_color(t::focus_ring())
                    .rounded_md()
                    .p_2()
                    .child(dialog.input.clone()),
            )
            .children(dialog.error.clone().map(|e| div().child(e)))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("go-cancel")
                            .label("Cancel")
                            .on_click(cx.listener(|b, _, w, cx| {
                                b.commands.go_to = None;
                                w.focus(&b.focus, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("go-confirm")
                            .label("Go")
                            .on_click(cx.listener(|b, _, w, cx| b.confirm_go_to(w, cx))),
                    ),
            );
        Some(
            gpui_kit::deferred(
                div()
                    .absolute()
                    .size_full()
                    .bg(gpui_kit::rgba(0x00000030))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(panel),
            )
            .with_priority(5)
            .into_any_element(),
        )
    }
}
