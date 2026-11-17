// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use addresses_rs::database::{AddressBook, Contact, ContactFields};
use anyhow::Result;
use gpui::{Context, Entity, div, prelude::*, px, rgb};

use super::super::contact_export::ExportFormat;
use super::super::duplicate_matching::{DuplicateMatch, find_duplicate_matches};
use super::{AddressBookApp, Theme};
use crate::text_field::TextField;

impl AddressBookApp {
    pub(super) fn open_duplicate_review(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        if self.show_duplicate_review {
            self.show_duplicate_review = false;
            self.status_message = None;
            cx.notify();
            return;
        }

        let Some(database_path) = self.database_path.clone() else {
            self.set_status("Open a database before reviewing duplicate contacts.", true);
            cx.notify();
            return;
        };

        let executor = cx.background_executor().clone();
        self.is_loading = true;
        self.show_duplicate_review = true;
        self.duplicate_matches.clear();
        self.selected_contact_ids.clear();
        self.confirm_delete_selection = false;
        self.show_move_contact_targets = false;
        self.status_message = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let database = addresses_rs::database::Database::open_existing(database_path)?;
                    let contacts = database.search_contacts(None, "")?;
                    Ok(find_duplicate_matches(&contacts))
                })
                .await;

            if let Err(error) = this.update(cx, |this, cx| {
                this.finish_duplicate_review(result, cx);
            }) {
                eprintln!("Could not update the app after duplicate review: {error:#}");
            }
        })
        .detach();
    }

    fn finish_duplicate_review(
        &mut self,
        result: Result<Vec<DuplicateMatch>>,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        match result {
            Ok(matches) => {
                self.duplicate_matches = matches;
                self.status_message = None;
            }
            Err(error) => {
                eprintln!("Could not find likely duplicate contacts: {error:#}");
                self.show_duplicate_review = false;
                self.set_status(
                    "Duplicate contacts could not be checked. Check that the database is readable and try again.",
                    true,
                );
            }
        }
        cx.notify();
    }

    fn open_duplicate_contact(
        &mut self,
        contact: Contact,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.show_duplicate_review = false;
        self.open_contact_editor(Some(contact), window, cx);
    }

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

        if self.import_preview.is_some() {
            return self
                .render_contact_import_preview(theme, cx)
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
            .flex_none()
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
                                            if this.is_loading {
                                                return;
                                            }
                                            this.begin_address_book_creation(window, cx);
                                            cx.notify();
                                        }))
                                        .child("New address book"),
                                )
                                .child(
                                    div()
                                        .id("manage-address-books-button")
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(theme.panel_border))
                                        .text_sm()
                                        .text_color(rgb(theme.content_text))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if this.is_loading {
                                                return;
                                            }
                                            this.manage_address_books = !this.manage_address_books;
                                            this.confirm_delete_address_book = None;
                                            cx.notify();
                                        }))
                                        .child(if self.manage_address_books {
                                            "Done"
                                        } else {
                                            "Manage books"
                                        }),
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
                                            if this.is_loading {
                                                return;
                                            }
                                            this.open_contact_editor(None, window, cx);
                                            cx.notify();
                                        }))
                                        .child("New contact"),
                                )
                                .child(
                                    div()
                                        .id("import-contacts-button")
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(theme.panel_border))
                                        .text_sm()
                                        .text_color(rgb(theme.content_text))
                                        .when(self.is_loading, |this| this.opacity(0.6))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.prompt_import_contacts(cx);
                                        }))
                                        .child("Import CSV"),
                                )
                                .when(!self.address_books.is_empty(), |this| {
                                    this.child(
                                    div()
                                        .id("review-duplicates-button")
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(theme.panel_border))
                                        .text_sm()
                                        .text_color(rgb(theme.content_text))
                                        .when(self.is_loading, |this| this.opacity(0.6))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_duplicate_review(cx);
                                        }))
                                        .child(if self.show_duplicate_review {
                                            "Show contacts"
                                        } else {
                                            "Review duplicates"
                                        }),
                                    )
                                })
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
            .when(!self.show_duplicate_review, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(address_book_filters),
                )
            })
            .when(self.manage_address_books && !self.show_duplicate_review, |this| {
                this.child(self.render_address_book_manager(theme, cx))
            })
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
                this
                    .when(!self.show_duplicate_review, |this| {
                        this.child(
                            div()
                                .id("contact-search")
                                .child(self.search_field.clone()),
                        )
                    })
                    .when(
                        !self.show_duplicate_review && !self.selected_contact_ids.is_empty(),
                        |this| this.child(self.render_contact_bulk_actions(theme, cx)),
                    )
                    .when(
                        !self.show_duplicate_review
                            && self.show_move_contact_targets
                            && !self.selected_contact_ids.is_empty(),
                        |this| this.child(self.render_contact_move_targets(theme, cx)),
                    )
                    .when(self.show_duplicate_review, |this| {
                        this.child(self.render_duplicate_review(theme, cx))
                    })
                    .when(!self.show_duplicate_review, |this| {
                        this.child(
                            div()
                                .id("contact-results")
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .children(
                                    self.contacts
                                        .iter()
                                        .map(|contact| {
                                            self.render_contact_item(contact, theme, cx)
                                        })
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
            })
            .into_any_element()
    }

    fn render_duplicate_review(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let match_count = self.duplicate_matches.len();
        div()
            .id("duplicate-review")
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted_text))
                    .child(
                        "Potential matches are found across all address books using shared email addresses or phone numbers, exact names, and similar names with a shared organization or postal address. Matches are suggestions only; contacts are never changed automatically.",
                    ),
            )
            .when(self.is_loading, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted_text))
                        .child("Checking contacts for likely duplicates…"),
                )
            })
            .when(!self.is_loading && match_count == 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted_text))
                        .child("No likely duplicate contacts found."),
                )
            })
            .when(!self.is_loading && match_count > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .child(format!("{match_count} possible duplicate pair(s) found.")),
                )
                .children(
                    self.duplicate_matches
                        .iter()
                        .enumerate()
                        .map(|(index, duplicate)| {
                            self.render_duplicate_match(index, duplicate, theme, cx)
                        }),
                )
            })
    }

    fn render_duplicate_match(
        &self,
        index: usize,
        duplicate: &DuplicateMatch,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(("duplicate-match", index as u64))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme.panel_border))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted_text))
                    .child(duplicate.reasons.join(" · ")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .child(self.render_duplicate_contact(index * 2, &duplicate.first, theme, cx))
                    .child(self.render_duplicate_contact(
                        index * 2 + 1,
                        &duplicate.second,
                        theme,
                        cx,
                    )),
            )
    }

    fn render_duplicate_contact(
        &self,
        element_id: usize,
        contact: &Contact,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let contact_to_edit = contact.clone();
        let details = contact_card_details(&contact.fields);
        let address_book_name = self
            .address_books
            .iter()
            .find(|book| book.id == contact.address_book_id)
            .map_or("Unknown address book", |book| book.name.as_str())
            .to_owned();

        div()
            .id(("duplicate-contact", element_id as u64))
            .flex_1()
            .min_w(px(220.))
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().child(contact_card_name(&contact.fields)))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted_text))
                    .child(address_book_name),
            )
            .children(details.into_iter().map(|detail| {
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted_text))
                    .child(detail)
            }))
            .child(
                div()
                    .id(("edit-duplicate-contact", element_id as u64))
                    .w(px(120.))
                    .mt_1()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(theme.panel_border))
                    .text_sm()
                    .when(self.is_loading, |this| this.opacity(0.6))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_duplicate_contact(contact_to_edit.clone(), window, cx);
                        cx.notify();
                    }))
                    .child("Edit contact"),
            )
    }

    fn render_contact_import_preview(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let preview = self
            .import_preview
            .as_ref()
            .expect("contact import preview is shown only while it exists");
        let source = preview.parsed.source.label();
        let contact_count = preview.parsed.contacts.len();
        let skipped_rows = preview.parsed.skipped_empty_rows;
        let extra_field_names = if preview.parsed.extra_field_names.is_empty() {
            "All non-empty columns map to supported fields.".to_owned()
        } else {
            format!(
                "Unmapped columns will be preserved: {}{}",
                preview
                    .parsed
                    .extra_field_names
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
                if preview.parsed.extra_field_names.len() > 5 {
                    ", …"
                } else {
                    ""
                }
            )
        };
        let sample_contacts = preview
            .parsed
            .contacts
            .iter()
            .take(5)
            .map(|contact| {
                let name = contact.display_name();
                if name.trim().is_empty() {
                    "Unnamed contact".to_owned()
                } else {
                    name
                }
            })
            .collect::<Vec<_>>();
        let address_book_options = self
            .address_books
            .iter()
            .map(|address_book| {
                let id = address_book.id;
                let is_selected = preview.target_address_book == Some(id);
                div()
                    .id(("import-target-book", id as u64))
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
                        this.select_import_address_book(id, cx);
                    }))
                    .child(address_book.name.clone())
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_xl().child("Import contacts"))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted_text))
                    .child(format!(
                        "{} contacts found in {} ({source}).",
                        contact_count, preview.file_name
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(sample_contacts.into_iter().map(|name| {
                        div()
                            .text_sm()
                            .text_color(rgb(theme.content_text))
                            .child(name)
                    })),
            )
            .when(skipped_rows > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme.muted_text))
                        .child(format!(
                            "{skipped_rows} completely empty row(s) will be skipped."
                        )),
                )
            })
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted_text))
                    .child(extra_field_names),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.muted_text))
                    .child(
                        "Duplicate detection is not performed. Importing the same contacts more than once creates duplicates.",
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.content_text))
                    .child("Import into address book:"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(address_book_options),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .mt_2()
                    .when(preview.target_address_book.is_some(), |this| {
                        this.child(
                            div()
                                .id("confirm-contact-import")
                                .px_4()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(theme.accent))
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.import_contacts(cx);
                                }))
                                .child(if self.is_loading {
                                    "Importing…"
                                } else {
                                    "Import contacts"
                                }),
                        )
                    })
                    .when(preview.target_address_book.is_none(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme.muted_text))
                                .child("Select an address book to continue."),
                        )
                    })
                    .child(
                        div()
                            .id("cancel-contact-import")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(theme.panel_border))
                            .text_sm()
                            .when(self.is_loading, |this| this.opacity(0.6))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_contact_import(cx);
                            }))
                            .child("Cancel"),
                    ),
            )
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

    fn render_address_book_manager(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("address-book-manager")
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme.panel_border))
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child("Manage address books"))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted_text))
                    .child("Only empty address books can be deleted. Move their contacts first."),
            )
            .children(
                self.address_books
                    .iter()
                    .map(|book| self.render_address_book_manager_row(book, theme, cx)),
            )
    }

    fn render_address_book_manager_row(
        &self,
        book: &AddressBook,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = book.id;
        let is_confirming = self.confirm_delete_address_book == Some(id);

        div()
            .id(("address-book-manager-row", id as u64))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(div().text_sm().child(book.name.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(is_confirming, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme.muted_text))
                                .child("Delete only if this book is empty?"),
                        )
                        .child(
                            div()
                                .id(("cancel-delete-address-book", id as u64))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.panel_border))
                                .text_sm()
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_delete_address_book(cx);
                                }))
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id(("confirm-delete-address-book", id as u64))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(0xb42318))
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.delete_address_book(id, cx);
                                }))
                                .child("Delete"),
                        )
                    })
                    .when(!is_confirming, |this| {
                        this.child(
                            div()
                                .id(("delete-address-book", id as u64))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(0xb42318))
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.request_delete_address_book(id, cx);
                                }))
                                .child("Delete"),
                        )
                    }),
            )
    }

    fn render_contact_row(
        &self,
        contact: &Contact,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let name = contact_card_name(&contact.fields);
        let address_book_name = self
            .address_books
            .iter()
            .find(|book| book.id == contact.address_book_id)
            .map_or("Unknown address book", |book| book.name.as_str())
            .to_owned();
        let details = contact_card_details(&contact.fields);
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
                            .when(details.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(theme.muted_text))
                                        .child("No additional details"),
                                )
                            })
                            .children(details.into_iter().map(|detail| {
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme.muted_text))
                                    .child(detail)
                            })),
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
                "Display name".to_owned(),
                editor.name.clone(),
            ))
            .child(Self::render_editor_field(
                "Given name".to_owned(),
                editor.given_name.clone(),
            ))
            .child(Self::render_editor_field(
                "Family name".to_owned(),
                editor.family_name.clone(),
            ))
            .child(Self::render_editor_field(
                "Organization".to_owned(),
                editor.company.clone(),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child("Email addresses"))
                            .child(
                                div()
                                    .id("add-email-field")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_email_field(cx);
                                    }))
                                    .child("Add email"),
                            ),
                    )
                    .children(editor.emails.iter().enumerate().map(|(index, email)| {
                        let label = email.label.as_deref().map_or_else(
                            || format!("Email address {}", index + 1),
                            |label| format!("Email ({label})"),
                        );
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .child(Self::render_editor_field(label, email.field.clone())),
                            )
                            .child(
                                div()
                                    .id(("remove-email-field", index as u64))
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_email_field(index, cx);
                                    }))
                                    .child("Remove"),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child("Phone numbers"))
                            .child(
                                div()
                                    .id("add-phone-field")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_phone_field(cx);
                                    }))
                                    .child("Add phone"),
                            ),
                    )
                    .children(editor.phones.iter().enumerate().map(|(index, phone)| {
                        let label = phone.label.as_deref().map_or_else(
                            || format!("Phone number {}", index + 1),
                            |label| format!("Phone ({label})"),
                        );
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .child(Self::render_editor_field(label, phone.field.clone())),
                            )
                            .child(
                                div()
                                    .id(("remove-phone-field", index as u64))
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_phone_field(index, cx);
                                    }))
                                    .child("Remove"),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child("Addresses"))
                            .child(
                                div()
                                    .id("add-address-field")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_address_field(cx);
                                    }))
                                    .child("Add address"),
                            ),
                    )
                    .children(editor.addresses.iter().enumerate().map(|(index, address)| {
                        let label = address
                            .original
                            .label
                            .as_deref()
                            .filter(|label| !label.trim().is_empty())
                            .map_or_else(
                                || "Address".to_owned(),
                                |label| format!("{label} address"),
                            );
                        div()
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(theme.panel_border))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_sm().child(label))
                                    .child(
                                        div()
                                            .id(("remove-address-field", index as u64))
                                            .px_3()
                                            .py_2()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(theme.panel_border))
                                            .text_sm()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.remove_address_field(index, cx);
                                            }))
                                            .child("Remove"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_3()
                                    .child(div().flex_1().child(Self::render_editor_field(
                                        "Street".to_owned(),
                                        address.street.clone(),
                                    )))
                                    .child(div().w(px(140.)).child(Self::render_editor_field(
                                        "House number".to_owned(),
                                        address.house_number.clone(),
                                    ))),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_3()
                                    .child(div().flex_1().child(Self::render_editor_field(
                                        "Postal code".to_owned(),
                                        address.postal_code.clone(),
                                    )))
                                    .child(div().flex_1().child(Self::render_editor_field(
                                        "City".to_owned(),
                                        address.city.clone(),
                                    ))),
                            )
                            .child(Self::render_editor_field(
                                "Country".to_owned(),
                                address.country.clone(),
                            ))
                    })),
            )
            .child(Self::render_editor_field(
                "Notes".to_owned(),
                editor.notes.clone(),
            ))
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

    fn render_editor_field(label: String, field: Entity<TextField>) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_sm().child(label))
            .child(field)
    }

    fn render_contact_item(
        &self,
        contact: &Contact,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let contact_id = contact.id;
        let is_selected = self.selected_contact_ids.contains(&contact_id);

        div()
            .id(("contact-selection-row", contact_id as u64))
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .id(("contact-selection-checkbox", contact_id as u64))
                    .size(gpui::px(20.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(theme.panel_border))
                    .when(is_selected, |this| {
                        this.bg(rgb(theme.accent)).text_color(rgb(0xffffff))
                    })
                    .when(self.is_loading, |this| this.opacity(0.6))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_contact_selection(contact_id, cx);
                    }))
                    .child(if is_selected { "✓" } else { "" }),
            )
            .child(self.render_contact_row(contact, theme, cx))
    }

    fn render_contact_bulk_actions(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected_count = self.selected_contact_ids.len();
        let all_visible_contacts_selected = !self.contacts.is_empty()
            && self
                .contacts
                .iter()
                .all(|contact| self.selected_contact_ids.contains(&contact.id));
        let confirmation = format!(
            "Delete {selected_count} selected contact{}? This cannot be undone.",
            if selected_count == 1 { "" } else { "s" }
        );

        div()
            .id("contact-bulk-actions")
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme.panel_border))
            .child(div().text_sm().text_color(rgb(theme.content_text)).child(
                if self.confirm_delete_selection {
                    confirmation
                } else {
                    format!("{selected_count} contacts selected")
                },
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .when(self.confirm_delete_selection, |this| {
                        this.child(
                            div()
                                .id("cancel-delete-selected-contacts")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.panel_border))
                                .text_sm()
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.cancel_delete_selected_contacts(cx);
                                }))
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id("confirm-delete-selected-contacts")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(0xb42318))
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.delete_selected_contacts(cx);
                                }))
                                .child("Delete"),
                        )
                    })
                    .when(!self.confirm_delete_selection, |this| {
                        this.when(self.address_books.len() > 1, |this| {
                            this.child(
                                div()
                                    .id("move-selected-contacts")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .when(self.is_loading, |this| this.opacity(0.6))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_contact_move_targets(cx);
                                    }))
                                    .child("Move to…"),
                            )
                        })
                        .child(
                            div()
                                .id("export-selected-contacts-csv")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.panel_border))
                                .text_sm()
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.prompt_export_contacts(ExportFormat::Csv, cx);
                                }))
                                .child("Export CSV"),
                        )
                        .child(
                            div()
                                .id("export-selected-contacts-json")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.panel_border))
                                .text_sm()
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.prompt_export_contacts(ExportFormat::Json, cx);
                                }))
                                .child("Export JSON"),
                        )
                        .when(!all_visible_contacts_selected, |this| {
                            this.child(
                                div()
                                    .id("select-all-contacts")
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(theme.panel_border))
                                    .text_sm()
                                    .when(self.is_loading, |this| this.opacity(0.6))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.select_all_visible_contacts(cx);
                                    }))
                                    .child("Select all"),
                            )
                        })
                        .child(
                            div()
                                .id("clear-contact-selection")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.panel_border))
                                .text_sm()
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_contact_selection(cx);
                                }))
                                .child("Clear selection"),
                        )
                        .child(
                            div()
                                .id("delete-selected-contacts")
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(0xb42318))
                                .when(self.is_loading, |this| this.opacity(0.6))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.request_delete_selected_contacts(cx);
                                }))
                                .child("Delete selected"),
                        )
                    }),
            )
    }

    fn render_contact_move_targets(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("contact-move-targets")
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme.panel_border))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(theme.content_text))
                    .child("Move selected contacts to:"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(self.address_books.iter().map(|book| {
                        let address_book_id = book.id;
                        div()
                            .id(("move-contacts-to-address-book", address_book_id as u64))
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(theme.panel_border))
                            .text_sm()
                            .when(self.is_loading, |this| this.opacity(0.6))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.move_selected_contacts(address_book_id, cx);
                            }))
                            .child(book.name.clone())
                    }))
                    .child(
                        div()
                            .id("cancel-move-contact-selection")
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .text_sm()
                            .text_color(rgb(theme.muted_text))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_contact_move_targets(cx);
                            }))
                            .child("Cancel"),
                    ),
            )
    }

    fn toggle_contact_selection(&mut self, contact_id: i64, cx: &mut Context<Self>) {
        if self.is_loading || !self.contacts.iter().any(|contact| contact.id == contact_id) {
            return;
        }

        if !self.selected_contact_ids.insert(contact_id) {
            self.selected_contact_ids.remove(&contact_id);
        }
        self.confirm_delete_selection = false;
        if self.selected_contact_ids.is_empty() {
            self.show_move_contact_targets = false;
        }
        cx.notify();
    }

    fn clear_contact_selection(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        self.selected_contact_ids.clear();
        self.confirm_delete_selection = false;
        self.show_move_contact_targets = false;
        cx.notify();
    }

    fn select_all_visible_contacts(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        self.selected_contact_ids
            .extend(self.contacts.iter().map(|contact| contact.id));
        self.confirm_delete_selection = false;
        cx.notify();
    }

    fn open_contact_move_targets(&mut self, cx: &mut Context<Self>) {
        if self.is_loading || self.selected_contact_ids.is_empty() {
            return;
        }
        self.show_move_contact_targets = true;
        self.confirm_delete_selection = false;
        cx.notify();
    }

    fn close_contact_move_targets(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        self.show_move_contact_targets = false;
        cx.notify();
    }

    fn move_selected_contacts(&mut self, address_book_id: i64, cx: &mut Context<Self>) {
        if self.is_loading || !self.show_move_contact_targets {
            return;
        }
        let Some(address_book_name) = self
            .address_books
            .iter()
            .find(|book| book.id == address_book_id)
            .map(|book| book.name.clone())
        else {
            self.set_status("Choose an existing destination address book.", true);
            self.show_move_contact_targets = false;
            cx.notify();
            return;
        };
        let Some(database_path) = self.database_path.clone() else {
            self.set_status("Open a database before moving contacts.", true);
            cx.notify();
            return;
        };
        let contact_ids = self
            .selected_contact_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        if contact_ids.is_empty() {
            self.show_move_contact_targets = false;
            cx.notify();
            return;
        }

        let executor = cx.background_executor().clone();
        self.is_loading = true;
        self.show_move_contact_targets = false;
        self.set_status(
            &format!(
                "Moving {} selected contacts to {address_book_name}…",
                contact_ids.len()
            ),
            false,
        );
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let database = addresses_rs::database::Database::open_existing(database_path)?;
                    database.move_contacts(&contact_ids, address_book_id)
                })
                .await;

            if let Err(error) = this.update(cx, |this, cx| {
                this.finish_move_selected_contacts(result, address_book_name, cx);
            }) {
                eprintln!("Could not update the app after moving contacts: {error:#}");
            }
        })
        .detach();
    }

    fn finish_move_selected_contacts(
        &mut self,
        result: Result<usize>,
        address_book_name: String,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        match result {
            Ok(moved_count) => {
                self.selected_contact_ids.clear();
                self.confirm_delete_selection = false;
                self.show_move_contact_targets = false;
                self.refresh_contacts(cx);
                if !self
                    .status_message
                    .as_ref()
                    .is_some_and(|message| message.is_error)
                {
                    let message = if moved_count == 0 {
                        format!("No selected contacts needed moving to {address_book_name}.")
                    } else {
                        format!(
                            "Moved {moved_count} contact{} to {address_book_name}.",
                            if moved_count == 1 { "" } else { "s" }
                        )
                    };
                    self.set_status(&message, false);
                }
            }
            Err(error) => {
                eprintln!("Could not move selected contacts: {error:#}");
                self.show_move_contact_targets = true;
                self.set_status(
                    "The selected contacts could not be moved. Check that the database is writable and try again.",
                    true,
                );
            }
        }
        cx.notify();
    }

    fn request_delete_address_book(&mut self, address_book_id: i64, cx: &mut Context<Self>) {
        if self.is_loading
            || !self
                .address_books
                .iter()
                .any(|book| book.id == address_book_id)
        {
            return;
        }
        let Some(database) = self.database.as_ref() else {
            self.set_status("Open a database before deleting an address book.", true);
            cx.notify();
            return;
        };
        match database.count_contacts(Some(address_book_id)) {
            Ok(contact_count) if contact_count > 0 => {
                self.confirm_delete_address_book = None;
                self.set_status(
                    &format!(
                        "Move or delete its {contact_count} contact{} before deleting the address book.",
                        if contact_count == 1 { "" } else { "s" }
                    ),
                    true,
                );
                cx.notify();
                return;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("Could not check address book contents: {error:#}");
                self.set_status(
                    "The address book could not be checked. Try again after verifying the database.",
                    true,
                );
                cx.notify();
                return;
            }
        }
        self.confirm_delete_address_book = Some(address_book_id);
        cx.notify();
    }

    fn cancel_delete_address_book(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        self.confirm_delete_address_book = None;
        cx.notify();
    }

    fn delete_address_book(&mut self, address_book_id: i64, cx: &mut Context<Self>) {
        if self.is_loading || self.confirm_delete_address_book != Some(address_book_id) {
            return;
        }
        let Some(address_book_name) = self
            .address_books
            .iter()
            .find(|book| book.id == address_book_id)
            .map(|book| book.name.clone())
        else {
            self.confirm_delete_address_book = None;
            cx.notify();
            return;
        };
        let Some(database_path) = self.database_path.clone() else {
            self.set_status("Open a database before deleting an address book.", true);
            cx.notify();
            return;
        };

        let executor = cx.background_executor().clone();
        self.is_loading = true;
        self.set_status(
            &format!("Deleting address book {address_book_name}…"),
            false,
        );
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let database = addresses_rs::database::Database::open_existing(database_path)?;
                    database.delete_address_book(address_book_id)
                })
                .await;

            if let Err(error) = this.update(cx, |this, cx| {
                this.finish_delete_address_book(result, address_book_name, cx);
            }) {
                eprintln!("Could not update the app after deleting an address book: {error:#}");
            }
        })
        .detach();
    }

    fn finish_delete_address_book(
        &mut self,
        result: Result<bool>,
        address_book_name: String,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        self.confirm_delete_address_book = None;
        match result {
            Ok(true) => {
                self.refresh_contacts(cx);
                self.refresh_dashboard_counts();
                if !self
                    .status_message
                    .as_ref()
                    .is_some_and(|message| message.is_error)
                {
                    self.set_status(
                        &format!("Deleted empty address book {address_book_name}."),
                        false,
                    );
                }
            }
            Ok(false) => {
                self.set_status(
                    "Address books can only be deleted when empty. Move or delete their contacts first.",
                    true,
                );
            }
            Err(error) => {
                eprintln!("Could not delete address book: {error:#}");
                self.set_status(
                    "The address book could not be deleted. Check that the database is writable and try again.",
                    true,
                );
            }
        }
        cx.notify();
    }

    fn request_delete_selected_contacts(&mut self, cx: &mut Context<Self>) {
        if self.is_loading || self.selected_contact_ids.is_empty() {
            return;
        }
        self.confirm_delete_selection = true;
        self.show_move_contact_targets = false;
        cx.notify();
    }

    fn cancel_delete_selected_contacts(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        self.confirm_delete_selection = false;
        cx.notify();
    }

    fn delete_selected_contacts(&mut self, cx: &mut Context<Self>) {
        if self.is_loading || !self.confirm_delete_selection {
            return;
        }
        let Some(database_path) = self.database_path.clone() else {
            self.set_status("Open a database before deleting contacts.", true);
            cx.notify();
            return;
        };
        let contact_ids = self
            .selected_contact_ids
            .iter()
            .copied()
            .collect::<Vec<_>>();
        if contact_ids.is_empty() {
            self.confirm_delete_selection = false;
            cx.notify();
            return;
        }

        let executor = cx.background_executor().clone();
        self.is_loading = true;
        self.set_status(
            &format!("Deleting {} selected contacts…", contact_ids.len()),
            false,
        );
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let database = addresses_rs::database::Database::open_existing(database_path)?;
                    database.delete_contacts(&contact_ids)
                })
                .await;

            if let Err(error) = this.update(cx, |this, cx| {
                this.finish_delete_selected_contacts(result, cx);
            }) {
                eprintln!("Could not update the app after deleting contacts: {error:#}");
            }
        })
        .detach();
    }

    fn finish_delete_selected_contacts(&mut self, result: Result<usize>, cx: &mut Context<Self>) {
        self.is_loading = false;
        match result {
            Ok(deleted_count) => {
                self.selected_contact_ids.clear();
                self.confirm_delete_selection = false;
                self.refresh_contacts(cx);
                self.refresh_dashboard_counts();
                if !self
                    .status_message
                    .as_ref()
                    .is_some_and(|message| message.is_error)
                {
                    let message = if deleted_count == 0 {
                        "No selected contacts were deleted.".to_owned()
                    } else {
                        format!(
                            "Deleted {deleted_count} selected contact{}.",
                            if deleted_count == 1 { "" } else { "s" }
                        )
                    };
                    self.set_status(&message, false);
                }
            }
            Err(error) => {
                eprintln!("Could not delete selected contacts: {error:#}");
                self.confirm_delete_selection = false;
                self.set_status(
                    "The selected contacts could not be deleted. Check that the database is writable and try again.",
                    true,
                );
            }
        }
        cx.notify();
    }
}

fn contact_card_name(fields: &ContactFields) -> String {
    let canonical_name = [
        fields.name.prefix.as_deref(),
        fields.name.given_name.as_deref(),
        fields.name.middle_name.as_deref(),
        fields.name.family_name.as_deref(),
        fields.name.suffix.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    if !canonical_name.is_empty() {
        return canonical_name;
    }

    fields
        .name
        .display_name
        .as_deref()
        .or(fields.name.file_as.as_deref())
        .or(fields.name.nickname.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Unnamed contact")
        .to_owned()
}

fn contact_card_details(fields: &ContactFields) -> Vec<String> {
    let mut details = Vec::new();
    for (kind, values) in [("Email", &fields.emails), ("Phone", &fields.phones)] {
        for value in values.iter().filter(|value| !value.value.trim().is_empty()) {
            let label = value
                .label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
                .map_or_else(|| kind.to_owned(), |label| format!("{kind} ({label})"));
            details.push(format!("{label}: {}", value.value.trim()));
        }
    }

    for address in &fields.addresses {
        let street = [address.street.as_deref(), address.house_number.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let locality = [address.postal_code.as_deref(), address.city.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let address_parts = [
            address.po_box.as_deref(),
            Some(street.as_str()),
            address.extended_address.as_deref(),
            Some(locality.as_str()),
            address.region.as_deref(),
            address.country.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
        if address_parts.is_empty() {
            continue;
        }

        let label = address
            .label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .map_or_else(|| "Address".to_owned(), |label| format!("{label} address"));
        details.push(format!("{label}: {}", address_parts.join(", ")));
    }
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    use addresses_rs::database::{ContactName, LabeledValue, PostalAddress};

    #[test]
    fn contact_card_prefers_all_canonical_name_parts() {
        let fields = ContactFields {
            name: ContactName {
                display_name: Some("Ada Lovelace".to_owned()),
                prefix: Some("Dr.".to_owned()),
                given_name: Some("Ada".to_owned()),
                middle_name: Some("Byron".to_owned()),
                family_name: Some("Lovelace".to_owned()),
                suffix: Some("Jr.".to_owned()),
                ..ContactName::default()
            },
            ..ContactFields::default()
        };

        assert_eq!(contact_card_name(&fields), "Dr. Ada Byron Lovelace Jr.");
    }

    #[test]
    fn contact_card_falls_back_to_display_name_without_canonical_parts() {
        let fields = ContactFields {
            name: ContactName {
                display_name: Some("Ada Lovelace".to_owned()),
                ..ContactName::default()
            },
            ..ContactFields::default()
        };

        assert_eq!(contact_card_name(&fields), "Ada Lovelace");
    }

    #[test]
    fn contact_card_shows_all_emails_phones_and_structured_addresses() {
        let fields = ContactFields {
            emails: vec![
                LabeledValue {
                    label: Some("Home".to_owned()),
                    value: "ada@home.example".to_owned(),
                },
                LabeledValue {
                    label: Some("Work".to_owned()),
                    value: "ada@work.example".to_owned(),
                },
            ],
            phones: vec![
                LabeledValue {
                    label: Some("Mobile".to_owned()),
                    value: "+1 555 0100".to_owned(),
                },
                LabeledValue {
                    label: Some("Work".to_owned()),
                    value: "+1 555 0199".to_owned(),
                },
            ],
            addresses: vec![
                PostalAddress {
                    label: Some("Home".to_owned()),
                    street: Some("Main Street".to_owned()),
                    house_number: Some("42".to_owned()),
                    postal_code: Some("12345".to_owned()),
                    city: Some("Amsterdam".to_owned()),
                    country: Some("Netherlands".to_owned()),
                    ..PostalAddress::default()
                },
                PostalAddress {
                    label: Some("Work".to_owned()),
                    street: Some("Corporate Way".to_owned()),
                    house_number: Some("8".to_owned()),
                    postal_code: Some("2011".to_owned()),
                    city: Some("Haarlem".to_owned()),
                    country: Some("Netherlands".to_owned()),
                    ..PostalAddress::default()
                },
            ],
            ..ContactFields::default()
        };

        assert_eq!(
            contact_card_details(&fields),
            [
                "Email (Home): ada@home.example",
                "Email (Work): ada@work.example",
                "Phone (Mobile): +1 555 0100",
                "Phone (Work): +1 555 0199",
                "Home address: Main Street 42, 12345 Amsterdam, Netherlands",
                "Work address: Corporate Way 8, 2011 Haarlem, Netherlands",
            ]
        );
    }
}
