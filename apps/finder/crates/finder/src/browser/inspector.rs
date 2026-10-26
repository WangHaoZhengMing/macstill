//! File information, preview thumbnails and icon cache.
use super::*;
use gpui_kit::component::scroll::ScrollableElement;

impl Browser {
    /// The picture for an entry at `px` pixels: our folder, else the
    /// shell's icon or thumbnail once loaded (the generic one meanwhile).
    pub(super) fn icon_source(
        &mut self,
        e: &Entry,
        px: u32,
        cx: &mut Context<Self>,
    ) -> gpui_kit::ImageSource {
        if self.in_applications() {
            if let Some(path) = crate::services::applications::icon(&e.path) {
                return path.into();
            }
        }
        if e.is_dir {
            return e.icon().into();
        }
        let key = icons::key(&e.path, px);
        match self.preview.icons.get(&key) {
            Some(Icon::Ready(img)) => return gpui_kit::ImageSource::Render(img.clone()),
            Some(_) => return e.icon().into(),
            None => {}
        }
        self.preview.icons.insert(key.clone(), Icon::Loading);
        let path = e.path.clone();
        cx.spawn(async move |this, cx| {
            let img = cx
                .background_spawn(async move {
                    icons::load(&path, px).map(crate::ui::components::render_image)
                })
                .await;
            let _ = this.update(cx, |b, cx| {
                b.preview
                    .icons
                    .insert(key, img.map_or(Icon::Failed, Icon::Ready));
                cx.notify();
            });
        })
        .detach();
        e.icon().into()
    }
    pub(super) fn inspector(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // Search results carry only what the index knows: stat for the rest.
        let e = self
            .selected_entry()
            .map(|e| {
                if e.created.is_none() && !self.in_trash() && !self.in_applications() {
                    ops::entry_from_path(e.path.clone()).unwrap_or(e)
                } else {
                    e
                }
            })
            .or_else(|| {
                if self.in_trash() || self.in_applications() {
                    None
                } else {
                    ops::entry_from_path(self.navigation.directory.clone())
                }
            });
        let mut col = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .px(px(16.))
            .pb(px(10.))
            .text_size(px(t::TEXT_SIZE));
        if let Some(e) = e {
            let info = |label: &'static str, value: String| {
                div()
                    .flex()
                    .justify_between()
                    .gap(px(8.))
                    .py(px(5.))
                    .border_b_1()
                    .border_color(t::separator())
                    .text_size(px(11.5))
                    .child(
                        div()
                            .flex_none()
                            .text_color(t::text_secondary())
                            .child(label),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_color(t::text())
                            .truncate()
                            .child(value),
                    )
            };
            let size = if e.is_dir {
                String::new()
            } else {
                format!(" - {}", files::format_size(&e))
            };
            let thumb = !e.is_dir && icons::has_thumbnail(&e.path);
            let src = self.icon_source(&e, 512, cx);
            col = col
                .child(
                    div()
                        .h(px(220.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if thumb {
                            div()
                                .p(px(6.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(t::separator())
                                .child(
                                    div()
                                        .w(px(230.))
                                        .h(px(196.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .overflow_hidden()
                                        .child(
                                            img(src)
                                                .size_full()
                                                .object_fit(gpui_kit::ObjectFit::Contain),
                                        ),
                                )
                                .into_any_element()
                        } else {
                            img(src).size(px(160.)).into_any_element()
                        }),
                )
                .child(
                    div()
                        .mt(px(14.))
                        .text_size(px(t::TITLE_SIZE))
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(e.name.clone()),
                )
                .child(
                    div()
                        .text_color(t::text_secondary())
                        .child(format!("{}{size}", e.kind)),
                )
                .child(
                    div()
                        .mt(px(14.))
                        .mb(px(2.))
                        .flex()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(gpui_kit::FontWeight::BOLD)
                                .child("Information"),
                        )
                        .child(
                            div()
                                .id("info-toggle")
                                .text_size(px(12.))
                                .text_color(t::accent())
                                .child(if self.preview.info_collapsed {
                                    "Show More"
                                } else {
                                    "Show Less"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preview.info_collapsed = !this.preview.info_collapsed;
                                    cx.notify();
                                })),
                        ),
                )
                .when_some(self.listing.trash_items.get(&e.path), |d, item| {
                    d.child(info(
                        "Original location",
                        item.original_path().display().to_string(),
                    ))
                })
                .when(!self.preview.info_collapsed, |d| {
                    d.child(info("Created", files::format_date_long(e.created)))
                        .child(info(
                            if self.in_trash() {
                                "Deleted"
                            } else {
                                "Modified"
                            },
                            files::format_date_long(e.modified),
                        ))
                        .child(info("Last opened", files::format_date_long(e.accessed)))
                })
                .when(self.preview.info_collapsed, |d| {
                    d.child(info(
                        if self.in_trash() {
                            "Deleted"
                        } else {
                            "Modified"
                        },
                        files::format_date_long(e.modified),
                    ))
                })
                .child(
                    div()
                        .mt(px(14.))
                        .mb(px(6.))
                        .font_weight(gpui_kit::FontWeight::BOLD)
                        .child("Tags"),
                )
                .child({
                    let tags = self.tags.of(&e.path).to_vec();
                    div()
                        .id("tag-field")
                        .px(px(6.))
                        .py(px(5.))
                        .rounded(px(5.))
                        .bg(t::bar_bg())
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .when(tags.is_empty(), |d| {
                            d.text_color(t::text_tertiary()).child("Add Tags…")
                        })
                        .children(tags.into_iter().map(|tag| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .px(px(6.))
                                .rounded(px(5.))
                                .bg(t::glass_pressed())
                                .child(div().size(px(8.)).rounded_full().bg(tag.color()))
                                .child(tag.name())
                        }))
                        .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                            this.open_tag_menu(menu_point(e), cx)
                        }))
                });
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(col.overflow_y_scrollbar().scroll_bounce(true))
            .child(
                div()
                    .h(px(52.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("more")
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(3.))
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(8.))
                            .hover(|s| s.bg(t::glass_pressed()))
                            .text_size(px(t::SMALL_SIZE))
                            .text_color(t::text_secondary())
                            .child(sym("ellipsis.circle", 15., t::text_secondary()))
                            .child("More…")
                            .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                                this.open_action_menu(menu_point(e), cx)
                            })),
                    ),
            )
    }
}
