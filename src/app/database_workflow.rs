// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::path::PathBuf;

use addresses_rs::database::Database;
use anyhow::{Context as _, Result, bail};
use directories::BaseDirs;
use gpui::{Context, PathPromptOptions};

use super::{AddressBookApp, INITIAL_ADDRESS_BOOK, Page, PreparedDatabase, StatusMessage};

impl AddressBookApp {
    pub(super) fn prompt_create_database(&mut self, cx: &mut Context<Self>) {
        let Some(directories) = BaseDirs::new() else {
            self.set_status(
                "Could not locate your home folder. Check your operating-system account settings and try again.",
                true,
            );
            cx.notify();
            return;
        };

        let receiver = cx.prompt_for_new_path(directories.home_dir(), Some("addresses.sqlite"));
        self.is_loading = true;
        self.set_status("Choose where to save the new database.", false);
        cx.notify();

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(path))) => {
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.begin_database_operation(path, true, false, cx)
                    }) {
                        eprintln!("Could not start database creation: {error:#}");
                    }
                }
                Ok(Ok(None)) => {
                    if let Err(error) =
                        this.update(cx, |this, cx| this.cancel_database_selection(cx))
                    {
                        eprintln!("Could not update the application after cancelling: {error:#}");
                    }
                }
                Ok(Err(error)) => {
                    eprintln!("Could not show the database save dialog: {error:#}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.set_status(
                            "The save dialog could not be opened. Check your desktop file-dialog support and try again.",
                            true,
                        );
                        this.is_loading = false;
                        cx.notify();
                    }) {
                        eprintln!("Could not report the save-dialog error: {update_error:#}");
                    }
                }
                Err(error) => {
                    eprintln!("The database save dialog was interrupted: {error}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.set_status(
                            "The save dialog was interrupted. Try creating the database again.",
                            true,
                        );
                        this.is_loading = false;
                        cx.notify();
                    }) {
                        eprintln!("Could not report the save-dialog error: {update_error:#}");
                    }
                }
            }
        })
        .detach();
    }

    pub(super) fn prompt_open_database(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open database".into()),
        });
        self.is_loading = true;
        self.set_status("Choose an existing Addresses database.", false);
        cx.notify();

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        if let Err(error) = this.update(cx, |this, cx| {
                            this.begin_database_operation(path, false, false, cx)
                        }) {
                            eprintln!("Could not start opening the database: {error:#}");
                        }
                    } else if let Err(error) =
                        this.update(cx, |this, cx| this.cancel_database_selection(cx))
                    {
                        eprintln!("Could not update the application after cancelling: {error:#}");
                    }
                }
                Ok(Ok(None)) => {
                    if let Err(error) =
                        this.update(cx, |this, cx| this.cancel_database_selection(cx))
                    {
                        eprintln!("Could not update the application after cancelling: {error:#}");
                    }
                }
                Ok(Err(error)) => {
                    eprintln!("Could not show the database open dialog: {error:#}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.set_status(
                            "The open dialog could not be opened. Check your desktop file-dialog support and try again.",
                            true,
                        );
                        this.is_loading = false;
                        cx.notify();
                    }) {
                        eprintln!("Could not report the open-dialog error: {update_error:#}");
                    }
                }
                Err(error) => {
                    eprintln!("The database open dialog was interrupted: {error}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.set_status(
                            "The open dialog was interrupted. Try opening the database again.",
                            true,
                        );
                        this.is_loading = false;
                        cx.notify();
                    }) {
                        eprintln!("Could not report the open-dialog error: {update_error:#}");
                    }
                }
            }
        })
        .detach();
    }

    fn cancel_database_selection(&mut self, cx: &mut Context<Self>) {
        self.is_loading = false;
        self.status_message = None;
        cx.notify();
    }

    pub(super) fn begin_database_operation(
        &mut self,
        path: PathBuf,
        create: bool,
        is_startup: bool,
        cx: &mut Context<Self>,
    ) {
        let path = if create {
            match normalize_new_database_path(path) {
                Ok(path) => path,
                Err(error) => {
                    eprintln!("Invalid database filename: {error:#}");
                    self.is_loading = false;
                    self.set_status(
                        "Choose a database filename ending in .sqlite, .sqlite3, or .db.",
                        true,
                    );
                    cx.notify();
                    return;
                }
            }
        } else {
            path
        };

        self.is_loading = true;
        self.set_status(
            if is_startup {
                "Reopening your last database…"
            } else if create {
                "Creating your database…"
            } else {
                "Opening your database…"
            },
            false,
        );
        cx.notify();

        let operation_path = path.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { prepare_database(operation_path, create) })
                .await;
            if let Err(error) = this.update(cx, |this, cx| {
                this.finish_database_operation(result, path, create, is_startup, cx)
            }) {
                eprintln!("Could not update the app after database operation: {error:#}");
            }
        })
        .detach();
    }

    fn finish_database_operation(
        &mut self,
        result: Result<PreparedDatabase>,
        requested_path: PathBuf,
        create: bool,
        is_startup: bool,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        match result {
            Ok(prepared) => {
                self.contact_count = prepared.contact_count;
                self.address_book_count = prepared.address_book_count;
                self.address_books = prepared.address_books;
                self.selected_address_book = None;
                self.database_path = Some(prepared.path.clone());
                self.database = Some(prepared.database);
                self.settings.set_last_database(Some(prepared.path.clone()));
                self.active_page = Page::Home;
                self.status_message = None;
                self.contacts.clear();

                if let Err(error) = self.settings.save() {
                    eprintln!("Database opened, but settings could not be saved: {error:#}");
                    self.set_status(
                        "The database is open, but its location could not be saved for next time. Check that the app's settings folder is writable.",
                        true,
                    );
                }
            }
            Err(error) => {
                eprintln!("Database operation failed: {error:#}");
                let message = if is_startup {
                    format!(
                        "Your last database could not be reopened at {}. Open an existing database or create a new one.",
                        requested_path.display()
                    )
                } else if create {
                    format!(
                        "Could not create a database at {}. Choose a new filename and make sure its folder is writable.",
                        requested_path.display()
                    )
                } else {
                    format!(
                        "Could not open {} as an Addresses database. Choose a valid Addresses database or create a new one.",
                        requested_path.display()
                    )
                };
                self.set_status(&message, true);
            }
        }
        cx.notify();
    }

    pub(super) fn set_status(&mut self, text: &str, is_error: bool) {
        self.status_message = Some(StatusMessage {
            text: text.to_owned(),
            is_error,
        });
    }
}

pub(super) fn normalize_new_database_path(mut path: PathBuf) -> Result<PathBuf> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    match extension {
        None => {
            path.set_extension("sqlite");
        }
        Some(extension)
            if ["sqlite", "sqlite3", "db"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported)) => {}
        Some(_) => bail!("Unsupported database file extension"),
    }
    Ok(path)
}

pub(super) fn prepare_database(path: PathBuf, create: bool) -> Result<PreparedDatabase> {
    let database = if create {
        let database = Database::create_new(&path)?;
        database
            .create_address_book(INITIAL_ADDRESS_BOOK)
            .context("Could not create the initial Personal address book")?;
        database
    } else {
        Database::open_existing(&path)?
    };

    let contact_count = database.count_contacts(None)?;
    let address_book_count = database.count_address_books()?;
    let address_books = database.list_address_books()?;

    Ok(PreparedDatabase {
        database,
        path,
        contact_count,
        address_book_count,
        address_books,
    })
}
