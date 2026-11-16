// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::sync::{Arc, LazyLock};

use gpui::{
    Context, Image, ImageFormat, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render, Window,
    WindowAppearance, canvas, div, img, point, prelude::*, px, rgb,
};

use super::{AddressBookApp, Page};

mod contacts;

static SIDEBAR_LOGO: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../../assets/addresses-icon.png").to_vec(),
    ))
});

#[derive(Clone, Copy)]
struct Theme {
    sidebar: u32,
    sidebar_selected: u32,
    sidebar_hover: u32,
    sidebar_text: u32,
    sidebar_secondary: u32,
    sidebar_unselected: u32,
    accent: u32,
    content: u32,
    content_text: u32,
    muted_text: u32,
    panel: u32,
    panel_border: u32,
    error_text: u32,
}

impl Theme {
    fn from_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self {
                sidebar: 0x111827,
                sidebar_selected: 0x29384f,
                sidebar_hover: 0x1f2c40,
                sidebar_text: 0xe7edf7,
                sidebar_secondary: 0xaab5c7,
                sidebar_unselected: 0xc5cedc,
                accent: 0x3978e5,
                content: 0x11151d,
                content_text: 0xe7edf7,
                muted_text: 0xaab5c7,
                panel: 0x1b2432,
                panel_border: 0x344154,
                error_text: 0xffb4a2,
            },
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self {
                sidebar: 0x172033,
                sidebar_selected: 0x2d3d59,
                sidebar_hover: 0x26344c,
                sidebar_text: 0xe7edf7,
                sidebar_secondary: 0xaab5c7,
                sidebar_unselected: 0xc5cedc,
                accent: 0x3978e5,
                content: 0xf5f7fb,
                content_text: 0x172033,
                muted_text: 0x647087,
                panel: 0xffffff,
                panel_border: 0xe1e6ef,
                error_text: 0xb42318,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum DatabaseAction {
    Create,
    Open,
}

impl AddressBookApp {
    fn render_sidebar(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(224.))
            .h_full()
            .flex()
            .flex_col()
            .gap_6()
            .p_4()
            .bg(rgb(theme.sidebar))
            .text_color(rgb(theme.sidebar_text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(img(SIDEBAR_LOGO.clone()).w(px(36.)).h(px(36.)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_lg().child("Addresses"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(theme.sidebar_secondary))
                                    .child("Offline address book"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.render_navigation_item(Page::Home, theme, cx))
                    .child(self.render_navigation_item(Page::Contacts, theme, cx))
                    .child(self.render_navigation_item(Page::PrintLabels, theme, cx))
                    .child(self.render_navigation_item(Page::Settings, theme, cx)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(theme.sidebar_secondary))
                    .child("Local data stays on this device"),
            )
    }

    fn render_navigation_item(
        &self,
        page: Page,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_selected = self.active_page == page;

        div()
            .id(page.title())
            .w_full()
            .h(px(42.))
            .px_3()
            .flex()
            .items_center()
            .rounded_md()
            .text_sm()
            .when(is_selected, |this| {
                this.bg(rgb(theme.sidebar_selected))
                    .text_color(rgb(0xffffff))
            })
            .when(!is_selected, |this| {
                this.text_color(rgb(theme.sidebar_unselected))
            })
            .hover(|this| this.bg(rgb(theme.sidebar_hover)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active_page = page;
                if page == Page::Contacts {
                    this.refresh_contacts(cx);
                }
                cx.notify();
            }))
            .child(page.title())
    }

    fn render_database_button(
        &self,
        label: &'static str,
        action: DatabaseAction,
        is_primary: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let background = if is_primary {
            theme.accent
        } else {
            theme.sidebar_selected
        };

        div()
            .id(label)
            .px_4()
            .py_3()
            .rounded_md()
            .text_sm()
            .text_color(rgb(0xffffff))
            .bg(rgb(background))
            .when(self.is_loading, |this| this.opacity(0.6))
            .hover(|this| this.opacity(0.85))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.is_loading {
                    return;
                }
                match action {
                    DatabaseAction::Create => this.prompt_create_database(cx),
                    DatabaseAction::Open => this.prompt_open_database(cx),
                }
            }))
            .child(label)
    }

    fn render_stat_card(label: &'static str, value: String, theme: Theme) -> impl IntoElement {
        div()
            .flex_1()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme.panel_border))
            .bg(rgb(theme.panel))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted_text))
                    .child(label),
            )
            .child(div().mt_2().text_2xl().child(value))
    }

    fn render_home_panel(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .when(self.database.is_none(), |this| {
                this.child(div().text_xl().child("Welcome to Addresses"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.muted_text))
                            .child("Create a local database to get started, or open an existing Addresses database."),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(self.render_database_button(
                                "Create new database",
                                DatabaseAction::Create,
                                true,
                                theme,
                                cx,
                            ))
                            .child(self.render_database_button(
                                "Open existing database",
                                DatabaseAction::Open,
                                false,
                                theme,
                                cx,
                            )),
                    )
            })
            .when(self.database.is_some(), |this| {
                let contact_count = self.contact_count.to_string();
                let address_book_count = self.address_book_count.to_string();
                let database_path = self
                    .database_path
                    .as_ref()
                    .map_or_else(String::new, |path| path.display().to_string());

                this.child(
                    div()
                        .flex()
                        .gap_4()
                        .child(Self::render_stat_card(
                            "Contacts",
                            contact_count,
                            theme,
                        ))
                        .child(Self::render_stat_card(
                            "Address books",
                            address_book_count,
                            theme,
                        )),
                )
                .child(
                    div()
                        .p_4()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(theme.panel_border))
                        .bg(rgb(theme.panel))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted_text))
                                .child("Current database"),
                        )
                        .child(div().mt_2().text_sm().child(database_path)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted_text))
                        .child("Duplicate review will be available when duplicate matching is implemented."),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(self.render_database_button(
                            "Create another database",
                            DatabaseAction::Create,
                            true,
                            theme,
                            cx,
                        ))
                        .child(self.render_database_button(
                            "Open another database",
                            DatabaseAction::Open,
                            false,
                            theme,
                            cx,
                        )),
                )
            })
    }

    fn render_settings_panel(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let database_path = self
            .database_path
            .as_deref()
            .or_else(|| self.settings.last_database())
            .map_or_else(
                || "No database has been selected yet.".to_owned(),
                |path| path.display().to_string(),
            );
        let database_label = if self.database.is_some() {
            "Current database"
        } else {
            "Last opened database"
        };

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(theme.panel_border))
                    .bg(rgb(theme.panel))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted_text))
                            .child(database_label),
                    )
                    .child(div().mt_2().text_sm().child(database_path))
                    .child(
                        div()
                            .mt_3()
                            .text_sm()
                            .text_color(rgb(theme.muted_text))
                            .child("The last successfully opened database is reopened automatically at startup."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.render_database_button(
                        "Create new database",
                        DatabaseAction::Create,
                        true,
                        theme,
                        cx,
                    ))
                    .child(self.render_database_button(
                        "Open existing database",
                        DatabaseAction::Open,
                        false,
                        theme,
                        cx,
                    )),
            )
            .child(
                div()
                    .p_4()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(theme.panel_border))
                    .bg(rgb(theme.panel))
                    .child(div().text_sm().child("Appearance"))
                    .child(
                        div()
                            .mt_2()
                            .text_sm()
                            .text_color(rgb(theme.muted_text))
                            .child("Follow the operating system's light or dark preference."),
                    ),
            )
    }

    fn render_content(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.active_page;
        let status_text = self
            .status_message
            .as_ref()
            .map_or_else(String::new, |message| message.text.clone());
        let status_color = self
            .status_message
            .as_ref()
            .map_or(theme.muted_text, |message| {
                if message.is_error {
                    theme.error_text
                } else {
                    theme.muted_text
                }
            });
        let show_status = self.status_message.is_some();

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .gap_6()
            .p_8()
            .bg(rgb(theme.content))
            .text_color(rgb(theme.content_text))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_2xl().child(page.title()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(theme.muted_text))
                            .child(page.description()),
                    ),
            )
            .child(
                div()
                    .id("content-panel-wrapper")
                    .flex_1()
                    .w_full()
                    .relative()
                    .child(
                        div()
                            .id("content-panel")
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .flex()
                            .flex_col()
                            .when(page == Page::Contacts, |this| {
                                this.overflow_y_scroll()
                                    .scrollbar_width(px(8.))
                                    .track_scroll(&self.content_scroll_handle)
                            })
                            .gap_4()
                            .p_6()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(theme.panel_border))
                            .bg(rgb(theme.panel))
                            .when(show_status, |this| {
                                this.child(
                                    div()
                                        .p_3()
                                        .rounded_md()
                                        .text_sm()
                                        .text_color(rgb(status_color))
                                        .child(status_text),
                                )
                            })
                            .when(page == Page::Home, |this| {
                                this.child(self.render_home_panel(theme, cx))
                            })
                            .when(page == Page::Settings, |this| {
                                this.child(self.render_settings_panel(theme, cx))
                            })
                            .when(page == Page::Contacts, |this| {
                                this.child(self.render_contacts_panel(theme, cx))
                            })
                            .when(page == Page::PrintLabels, |this| {
                                this.child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_sm()
                                        .text_color(rgb(theme.muted_text))
                                        .child(page.empty_state()),
                                )
                            }),
                    )
                    .when(page == Page::Contacts, |this| {
                        this.child(self.render_content_scrollbar(theme, cx))
                    }),
            )
    }

    fn render_content_scrollbar(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_bounds = self.content_scroll_handle.bounds();
        let viewport_height = viewport_bounds.size.height;
        let max_offset = self.content_scroll_handle.max_offset().height;
        let track_inset = px(4.);
        let track_height = (viewport_height - track_inset * 2.).max(px(0.));
        let thumb_height = if viewport_height > px(0.) && max_offset > px(0.) {
            (track_height * (viewport_height / (viewport_height + max_offset)))
                .max(px(28.))
                .min(track_height)
        } else {
            track_height
        };
        let thumb_range = (track_height - thumb_height).max(px(0.));
        let scroll_progress = if max_offset > px(0.) {
            (-self.content_scroll_handle.offset().y / max_offset).clamp(0., 1.)
        } else {
            0.
        };
        let thumb_top = track_inset + thumb_range * scroll_progress;
        let entity = cx.entity();
        let scroll_handle = self.content_scroll_handle.clone();

        div()
            .id("content-scrollbar")
            .absolute()
            .top_0()
            .bottom_0()
            .right_2()
            .w(px(10.))
            .when(max_offset > px(0.), |this| {
                this.child(
                    div()
                        .absolute()
                        .top(track_inset)
                        .bottom(track_inset)
                        .left(px(2.))
                        .right(px(2.))
                        .rounded_full()
                        .bg(rgb(theme.content))
                        .child(
                            div()
                                .id("content-scrollbar-thumb")
                                .absolute()
                                .top(thumb_top - track_inset)
                                .w_full()
                                .h(thumb_height)
                                .rounded_full()
                                .bg(rgb(theme.muted_text))
                                .hover(|this| this.bg(rgb(theme.accent)))
                                .child(
                                    canvas(
                                        |_, _, _| (),
                                        move |thumb_bounds, _, window, _| {
                                            window.on_mouse_event({
                                                let entity = entity.clone();
                                                move |event: &MouseDownEvent, _, _, cx| {
                                                    if !thumb_bounds.contains(&event.position) {
                                                        return;
                                                    }
                                                    entity.update(cx, |this, _| {
                                                        this.scrollbar_drag_offset = Some(
                                                            event.position.y
                                                                - thumb_bounds.origin.y,
                                                        );
                                                    });
                                                }
                                            });
                                            window.on_mouse_event({
                                                let entity = entity.clone();
                                                move |_: &MouseUpEvent, _, _, cx| {
                                                    entity.update(cx, |this, _| {
                                                        this.scrollbar_drag_offset = None;
                                                    });
                                                }
                                            });
                                            window.on_mouse_event({
                                                let entity = entity.clone();
                                                let scroll_handle = scroll_handle.clone();
                                                move |event: &MouseMoveEvent, _, _, cx| {
                                                    if !event.dragging() {
                                                        return;
                                                    }
                                                    let Some(drag_offset) =
                                                        entity.read(cx).scrollbar_drag_offset
                                                    else {
                                                        return;
                                                    };
                                                    if thumb_range == px(0.) {
                                                        return;
                                                    }

                                                    let thumb_position = (event.position.y
                                                        - viewport_bounds.origin.y
                                                        - drag_offset
                                                        - track_inset)
                                                        .clamp(px(0.), thumb_range);
                                                    let progress = thumb_position / thumb_range;
                                                    let offset = scroll_handle.offset();
                                                    scroll_handle.set_offset(point(
                                                        offset.x,
                                                        -max_offset * progress,
                                                    ));
                                                    cx.notify(entity.entity_id());
                                                }
                                            });
                                        },
                                    )
                                    .size_full(),
                                ),
                        ),
                )
            })
    }
}

impl Render for AddressBookApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::from_appearance(window.appearance());
        div()
            .id("addresses-app")
            .size_full()
            .flex()
            .flex_row()
            .child(self.render_sidebar(theme, cx))
            .child(self.render_content(theme, cx))
    }
}
