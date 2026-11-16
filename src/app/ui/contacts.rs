// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use addresses_rs::database::Contact;
use gpui::{Context, Entity, div, prelude::*, rgb};

use super::{AddressBookApp, Theme};
use crate::text_field::TextField;

impl AddressBookApp {
    pub(super) fn render_contacts_panel(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        if self.database.is_none() {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(theme.muted_text))
                .child("Open or create a database from Home or Settings to manage contacts.")
                .into_any_element();
        }

        if self.editor.is_some() {
            return self.render_contact_editor(theme, cx).into_any_element();
        }

        let mut address_book_filters =
            vec![self.render_address_book_filter(None, "All address books", theme, cx)];
        for book in &self.address_books {
            address_book_filters.push(self.render_address_book_filter(
                Some(book.id),
                &book.name,
                theme,
                cx,
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child("Address books"))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .when(self.address_book_name_field.is_none(), |this| {
                                this.child(
                                    div()
                                        .id("new-address-book-button")
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(theme.panel_border))
                                        .text_sm()
                                        .text_color(rgb(theme.content_text))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.begin_address_book_creation(window, cx);
                                            cx.notify();
                                        }))
                                        .child("New address book"),
                                )
                                .child(
                                    div()
                                        .id("new-contact-button")
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .text_sm()
                                        .text_color(rgb(0xffffff))
                                        .bg(rgb(theme.accent))
                                        .hover(|this| this.opacity(0.85))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_contact_editor(None, window, cx);
                                            cx.notify();
                                        }))
                                        .child("New contact"),
                                )
                            }),
                    ),
            )
            .when(self.address_book_name_field.is_some(), |this| {
                let name_field = self
                    .address_book_name_field
                    .as_ref()
                    .expect("address-book field is shown only while creating one")
                    .clone();
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1().child(name_field))
                        .child(
                            div()
                                .id("submit-new-address-book")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(theme.accent))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.create_address_book(cx);
                                    cx.notify();
                                }))
                                .child("Create address book"),
                        )
                        .child(
                            div()
                                .id("cancel-new-address-book")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.panel_border))
                                .text_sm()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_address_book_creation();
                                    cx.notify();
                                }))
                                .child("Cancel"),
                        ),
                )
            })
            .child(div().flex().flex_wrap().gap_2()            .children(address_book_filters))
            .when(self.address_books.is_empty(), |this| {
                this.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(theme.panel_border))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme.muted_text))
                                .child("This database has no address books yet."),
                        )
                        .when(self.address_book_name_field.is_none(), |this| {
                            this.child(
                                div()
                                    .id("create-address-book-button")
                                    .mt_2()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .text_sm()
                                    .text_color(rgb(0xffffff))
                                    .bg(rgb(theme.accent))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.create_default_address_book(cx);
                                        cx.notify();
                                    }))
                                    .child("Create Personal address book"),
                            )
                        }),
                )
            })
            .when(!self.address_books.is_empty(), |this| {
                this.child(
                    div()
                        .id("contact-search")
                        .child(self.search_field.clone()),
                )
                .child(
                    div()
                        .id("contact-results")
                        .flex_1()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(
                            self.contacts
                                .iter()
                                .map(|contact| self.render_contact_row(contact, theme, cx))
                                .collect::<Vec<_>>(),
                        )
                        .when(self.contacts.is_empty(), |this| {
                            let search_is_empty =
                                self.search_field.read(cx).content().trim().is_empty();
                            let empty_message = if search_is_empty {
                                "No contacts in this address book yet. Add a contact to get started."
                            } else {
                                "No contacts match your search."
                            };
                            this.child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_sm()
                                    .text_color(rgb(theme.muted_text))
                                    .child(empty_message),
                            )
                        }),
                )
            })
            .into_any_element()
    }

    fn render_address_book_filter(
        &self,
        id: Option<i64>,
        label: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_selected = self.selected_address_book == id;
        div()
            .id(("address-book-filter", id.unwrap_or(0) as u64))
            .px_3()
            .py_2()
            .rounded_md()
            .text_sm()
            .when(is_selected, |this| {
                this.bg(rgb(theme.sidebar_selected))
                    .text_color(rgb(0xffffff))
            })
            .when(!is_selected, |this| {
                this.border_1()
                    .border_color(rgb(theme.panel_border))
                    .text_color(rgb(theme.content_text))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_address_book(id, cx);
            }))
            .child(label.to_owned())
    }

    fn render_contact_row(
        &self,
        contact: &Contact,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let name = contact.fields.display_name();
        let name = if name.trim().is_empty() {
            "Unnamed contact".to_owned()
        } else {
            name
        };
        let address_book_name = self
            .address_books
            .iter()
            .find(|book| book.id == contact.address_book_id)
            .map_or("Unknown address book", |book| book.name.as_str())
            .to_owned();
        let details = [
            contact
                .fields
                .organization
                .company
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
            contact
                .fields
                .emails
                .first()
                .map(|value| value.value.as_str())
                .filter(|value| !value.trim().is_empty()),
            contact
                .fields
                .phones
                .first()
                .map(|value| value.value.as_str())
                .filter(|value| !value.trim().is_empty()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let contact_to_edit = contact.clone();

        div()
            .id(("contact-row", contact.id as u64))
            .w_full()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme.panel_border))
            .bg(rgb(theme.panel))
            .hover(|this| this.bg(rgb(theme.sidebar_hover)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_contact_editor(Some(contact_to_edit.clone()), window, cx);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_lg().child(name))
                            .child(div().text_sm().text_color(rgb(theme.muted_text)).child(
                                if details.is_empty() {
                                    "No additional details".to_owned()
                                } else {
                                    details
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme.muted_text))
                            .child(address_book_name),
                    ),
            )
    }

    fn render_contact_editor(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let editor = self
            .editor
            .as_ref()
            .expect("contact editor is shown only when its state exists");
        let title = if editor.original.is_some() {
            "Edit contact"
        } else {
            "New contact"
        };
        let address_book_name = self
            .address_books
            .iter()
            .find(|book| book.id == editor.address_book_id)
            .map_or("Unknown address book", |book| book.name.as_str())
            .to_owned();

        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_3()
            .child(div().text_xl().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted_text))
                    .child(format!("Address book: {address_book_name}")),
            )
            .child(Self::render_editor_field(
                "Display name",
                editor.name.clone(),
            ))
            .child(Self::render_editor_field(
                "Given name",
                editor.given_name.clone(),
            ))
            .child(Self::render_editor_field(
                "Family name",
                editor.family_name.clone(),
            ))
            .child(Self::render_editor_field(
                "Organization",
                editor.company.clone(),
            ))
            .child(Self::render_editor_field("Email", editor.email.clone()))
            .child(Self::render_editor_field("Phone", editor.phone.clone()))
            .child(Self::render_editor_field("Street", editor.street.clone()))
            .child(Self::render_editor_field("City", editor.city.clone()))
            .child(Self::render_editor_field("Country", editor.country.clone()))
            .child(Self::render_editor_field("Notes", editor.notes.clone()))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .mt_2()
                    .child(
                        div()
                            .id("save-contact-button")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(0xffffff))
                            .bg(rgb(theme.accent))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_contact(cx);
                            }))
                            .child("Save contact"),
                    )
                    .child(
                        div()
                            .id("cancel-contact-button")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(theme.panel_border))
                            .text_sm()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editor = None;
                                this.status_message = None;
                                cx.notify();
                            }))
                            .child("Cancel"),
                    ),
            )
    }

    fn render_editor_field(
        label: &'static str,
        field: Entity<TextField>,
    ) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().child(label))
            .child(field)
    }
}
