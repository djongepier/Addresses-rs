// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::{collections::BTreeSet, path::PathBuf};

use addresses_rs::{
    database::{AddressBook, Contact, Database, LabeledValue, PostalAddress},
    settings::AppSettings,
};
use anyhow::Context as _;
use gpui::{AppContext as _, Context, Entity, Pixels, ScrollHandle, Window};

use crate::text_field::TextField;

mod contact_export;
mod contact_import;
mod database_workflow;
mod duplicate_matching;
mod ui;

const INITIAL_ADDRESS_BOOK: &str = "Personal";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Home,
    Contacts,
    PrintLabels,
    Settings,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Contacts => "Contacts",
            Self::PrintLabels => "Print labels",
            Self::Settings => "Settings",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Home => "An overview of your addressbooks and contacts.",
            Self::Contacts => "Browse and manage contacts across your addressbooks.",
            Self::PrintLabels => "Create and manage A4 address label layouts.",
            Self::Settings => "Manage your database and application preferences.",
        }
    }

    fn empty_state(self) -> &'static str {
        match self {
            Self::Home => "",
            Self::Contacts => "Contacts will appear here once a database is open.",
            Self::PrintLabels => "Your saved label layouts will appear here.",
            Self::Settings => "",
        }
    }
}

struct StatusMessage {
    text: String,
    is_error: bool,
}

struct PreparedDatabase {
    database: Database,
    path: PathBuf,
    contact_count: i64,
    address_book_count: i64,
    address_books: Vec<AddressBook>,
}

struct ContactEditor {
    original: Option<Contact>,
    address_book_id: i64,
    next_dynamic_field_id: u64,
    name: Entity<TextField>,
    given_name: Entity<TextField>,
    family_name: Entity<TextField>,
    company: Entity<TextField>,
    emails: Vec<ContactValueEditor>,
    phones: Vec<ContactValueEditor>,
    addresses: Vec<ContactAddressEditor>,
    notes: Entity<TextField>,
}

struct ContactValueEditor {
    label: Option<String>,
    field: Entity<TextField>,
}

struct ContactAddressEditor {
    original: PostalAddress,
    street: Entity<TextField>,
    house_number: Entity<TextField>,
    postal_code: Entity<TextField>,
    city: Entity<TextField>,
    country: Entity<TextField>,
}

pub(crate) struct AddressBookApp {
    active_page: Page,
    settings: AppSettings,
    database: Option<Database>,
    database_path: Option<PathBuf>,
    contact_count: i64,
    address_book_count: i64,
    is_loading: bool,
    status_message: Option<StatusMessage>,
    address_books: Vec<AddressBook>,
    selected_address_book: Option<i64>,
    contacts: Vec<Contact>,
    selected_contact_ids: BTreeSet<i64>,
    show_duplicate_review: bool,
    duplicate_matches: Vec<duplicate_matching::DuplicateMatch>,
    confirm_delete_selection: bool,
    show_move_contact_targets: bool,
    manage_address_books: bool,
    confirm_delete_address_book: Option<i64>,
    import_preview: Option<contact_import::ContactImportPreview>,
    content_scroll_handle: ScrollHandle,
    scrollbar_drag_offset: Option<Pixels>,
    search_field: Entity<TextField>,
    address_book_name_field: Option<Entity<TextField>>,
    editor: Option<ContactEditor>,
}

impl AddressBookApp {
    pub(crate) fn new(
        settings: AppSettings,
        settings_error: Option<anyhow::Error>,
        search_field: Entity<TextField>,
    ) -> Self {
        let mut app = Self {
            active_page: Page::Home,
            settings,
            database: None,
            database_path: None,
            contact_count: 0,
            address_book_count: 0,
            is_loading: false,
            status_message: None,
            address_books: Vec::new(),
            selected_address_book: None,
            contacts: Vec::new(),
            selected_contact_ids: BTreeSet::new(),
            show_duplicate_review: false,
            duplicate_matches: Vec::new(),
            confirm_delete_selection: false,
            show_move_contact_targets: false,
            manage_address_books: false,
            confirm_delete_address_book: None,
            import_preview: None,
            content_scroll_handle: ScrollHandle::new(),
            scrollbar_drag_offset: None,
            search_field,
            address_book_name_field: None,
            editor: None,
        };

        if let Some(error) = settings_error {
            eprintln!("Could not load application settings: {error:#}");
            app.set_status(
                "Saved settings could not be read. You can still open or create a database; opening one will save fresh settings.",
                true,
            );
        }

        app
    }

    pub(crate) fn on_search_changed(&mut self, cx: &mut Context<Self>) {
        if self.active_page == Page::Contacts && self.editor.is_none() {
            self.refresh_contacts(cx);
        }
    }

    fn refresh_contacts(&mut self, cx: &mut Context<Self>) {
        let Some(database) = self.database.as_ref() else {
            self.contacts.clear();
            self.address_books.clear();
            self.selected_contact_ids.clear();
            self.confirm_delete_selection = false;
            self.show_move_contact_targets = false;
            return;
        };

        match database.list_address_books() {
            Ok(address_books) => {
                if self
                    .selected_address_book
                    .is_some_and(|id| !address_books.iter().any(|book| book.id == id))
                {
                    self.selected_address_book = None;
                }
                self.address_books = address_books;
                if self
                    .confirm_delete_address_book
                    .is_some_and(|id| !self.address_books.iter().any(|book| book.id == id))
                {
                    self.confirm_delete_address_book = None;
                }
                let query = self.search_field.read(cx).content().to_owned();
                match database.search_contacts(self.selected_address_book, &query) {
                    Ok(contacts) => {
                        let visible_contact_ids = contacts
                            .iter()
                            .map(|contact| contact.id)
                            .collect::<BTreeSet<_>>();
                        self.selected_contact_ids
                            .retain(|id| visible_contact_ids.contains(id));
                        if self.selected_contact_ids.is_empty() {
                            self.confirm_delete_selection = false;
                            self.show_move_contact_targets = false;
                        }
                        self.contacts = contacts;
                        if self
                            .status_message
                            .as_ref()
                            .is_some_and(|status| status.is_error)
                        {
                            self.status_message = None;
                        }
                    }
                    Err(error) => {
                        eprintln!("Could not load contacts: {error:#}");
                        self.set_status(
                            "Contacts could not be loaded. Check the database and try again.",
                            true,
                        );
                    }
                }
            }
            Err(error) => {
                eprintln!("Could not load address books: {error:#}");
                self.set_status(
                    "Address books could not be loaded. Check the database and try again.",
                    true,
                );
            }
        }
        cx.notify();
    }

    fn select_address_book(&mut self, address_book_id: Option<i64>, cx: &mut Context<Self>) {
        self.selected_address_book = address_book_id;
        self.editor = None;
        self.show_move_contact_targets = false;
        self.refresh_contacts(cx);
    }

    fn new_contact_text_field(
        cx: &mut Context<Self>,
        id: String,
        placeholder: &'static str,
        value: Option<String>,
    ) -> Entity<TextField> {
        let value = value.unwrap_or_default();
        cx.new(move |cx| TextField::new(cx, id, placeholder, value))
    }

    fn new_contact_value_editor(
        value: Option<&LabeledValue>,
        id: String,
        placeholder: &'static str,
        cx: &mut Context<Self>,
    ) -> ContactValueEditor {
        ContactValueEditor {
            label: value.and_then(|value| value.label.clone()),
            field: Self::new_contact_text_field(
                cx,
                id,
                placeholder,
                value.map(|value| value.value.clone()),
            ),
        }
    }

    fn new_contact_address_editor(
        address: PostalAddress,
        id: String,
        cx: &mut Context<Self>,
    ) -> ContactAddressEditor {
        ContactAddressEditor {
            street: Self::new_contact_text_field(
                cx,
                format!("{id}-street"),
                "Street",
                address.street.clone(),
            ),
            house_number: Self::new_contact_text_field(
                cx,
                format!("{id}-house-number"),
                "House number",
                address.house_number.clone(),
            ),
            postal_code: Self::new_contact_text_field(
                cx,
                format!("{id}-postal-code"),
                "Postal code",
                address.postal_code.clone(),
            ),
            city: Self::new_contact_text_field(
                cx,
                format!("{id}-city"),
                "City",
                address.city.clone(),
            ),
            country: Self::new_contact_text_field(
                cx,
                format!("{id}-country"),
                "Country",
                address.country.clone(),
            ),
            original: address,
        }
    }

    fn open_contact_editor(
        &mut self,
        contact: Option<Contact>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let address_book_id = contact
            .as_ref()
            .map(|contact| contact.address_book_id)
            .or(self.selected_address_book)
            .or_else(|| self.address_books.first().map(|book| book.id));
        let Some(address_book_id) = address_book_id else {
            self.set_status("Create an address book before adding contacts.", true);
            return;
        };

        let fields = contact
            .as_ref()
            .map(|contact| &contact.fields)
            .cloned()
            .unwrap_or_default();
        let name = fields.name.display_name.clone().unwrap_or_default();
        let given_name = fields.name.given_name.clone().unwrap_or_default();
        let family_name = fields.name.family_name.clone().unwrap_or_default();
        let company = fields.organization.company.clone().unwrap_or_default();
        let emails = if fields.emails.is_empty() {
            vec![Self::new_contact_value_editor(
                None,
                "contact-email-0".to_owned(),
                "Email address",
                cx,
            )]
        } else {
            fields
                .emails
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    Self::new_contact_value_editor(
                        Some(value),
                        format!("contact-email-{index}"),
                        "Email address",
                        cx,
                    )
                })
                .collect()
        };
        let phones = if fields.phones.is_empty() {
            vec![Self::new_contact_value_editor(
                None,
                "contact-phone-0".to_owned(),
                "Phone number",
                cx,
            )]
        } else {
            fields
                .phones
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    Self::new_contact_value_editor(
                        Some(value),
                        format!("contact-phone-{index}"),
                        "Phone number",
                        cx,
                    )
                })
                .collect()
        };
        let mut addresses = fields.addresses.clone();
        if !addresses.iter().any(|address| is_home_address(address)) {
            addresses.push(PostalAddress {
                label: Some("Home".to_owned()),
                ..PostalAddress::default()
            });
        }
        if !addresses.iter().any(|address| is_work_address(address)) {
            addresses.push(PostalAddress {
                label: Some("Work".to_owned()),
                ..PostalAddress::default()
            });
        }
        let addresses = addresses
            .into_iter()
            .enumerate()
            .map(|(index, address)| {
                Self::new_contact_address_editor(address, format!("contact-address-{index}"), cx)
            })
            .collect();
        let notes = fields.notes.clone().unwrap_or_default();
        self.editor = Some(ContactEditor {
            original: contact,
            address_book_id,
            next_dynamic_field_id: 0,
            name: cx.new(move |cx| TextField::new(cx, "contact-name", "Display name", name)),
            given_name: cx
                .new(move |cx| TextField::new(cx, "contact-given-name", "Given name", given_name)),
            family_name: cx.new(move |cx| {
                TextField::new(cx, "contact-family-name", "Family name", family_name)
            }),
            company: cx
                .new(move |cx| TextField::new(cx, "contact-company", "Organization", company)),
            emails,
            phones,
            addresses,
            notes: cx.new(move |cx| TextField::new(cx, "contact-notes", "Notes", notes)),
        });
        self.status_message = None;
        if let Some(editor) = &self.editor {
            editor.name.read(cx).focus(window);
        }
    }

    fn add_email_field(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let id = editor.next_dynamic_field_id;
        editor.next_dynamic_field_id += 1;
        editor.emails.push(Self::new_contact_value_editor(
            None,
            format!("contact-email-added-{id}"),
            "Email address",
            cx,
        ));
        cx.notify();
    }

    fn remove_email_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut()
            && index < editor.emails.len()
        {
            editor.emails.remove(index);
            cx.notify();
        }
    }

    fn add_phone_field(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let id = editor.next_dynamic_field_id;
        editor.next_dynamic_field_id += 1;
        editor.phones.push(Self::new_contact_value_editor(
            None,
            format!("contact-phone-added-{id}"),
            "Phone number",
            cx,
        ));
        cx.notify();
    }

    fn remove_phone_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut()
            && index < editor.phones.len()
        {
            editor.phones.remove(index);
            cx.notify();
        }
    }

    fn add_address_field(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let id = editor.next_dynamic_field_id;
        editor.next_dynamic_field_id += 1;
        let label = if !editor
            .addresses
            .iter()
            .any(|address| is_home_address(&address.original))
        {
            "Home"
        } else if !editor
            .addresses
            .iter()
            .any(|address| is_work_address(&address.original))
        {
            "Work"
        } else {
            "Other"
        };
        editor.addresses.push(Self::new_contact_address_editor(
            PostalAddress {
                label: Some(label.to_owned()),
                ..PostalAddress::default()
            },
            format!("contact-address-added-{id}"),
            cx,
        ));
        cx.notify();
    }

    fn remove_address_field(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut()
            && index < editor.addresses.len()
        {
            editor.addresses.remove(index);
            cx.notify();
        }
    }

    fn save_contact(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let mut fields = editor
            .original
            .as_ref()
            .map(|contact| contact.fields.clone())
            .unwrap_or_default();
        fields.name.display_name = optional_field(editor.name.read(cx).content());
        fields.name.given_name = optional_field(editor.given_name.read(cx).content());
        fields.name.family_name = optional_field(editor.family_name.read(cx).content());
        fields.organization.company = optional_field(editor.company.read(cx).content());
        fields.emails = non_empty_labeled_values(editor.emails.iter().map(|value| {
            (
                value.label.clone(),
                value.field.read(cx).content().to_owned(),
            )
        }));
        fields.phones = non_empty_labeled_values(editor.phones.iter().map(|value| {
            (
                value.label.clone(),
                value.field.read(cx).content().to_owned(),
            )
        }));
        if fields.display_name().trim().is_empty() {
            self.set_status("Enter a name or email address for this contact.", true);
            return;
        }

        fields.addresses = editor
            .addresses
            .iter()
            .filter_map(|address_editor| {
                let mut address = address_editor.original.clone();
                address.street = optional_field(address_editor.street.read(cx).content());
                address.house_number =
                    optional_field(address_editor.house_number.read(cx).content());
                address.postal_code = optional_field(address_editor.postal_code.read(cx).content());
                address.city = optional_field(address_editor.city.read(cx).content());
                address.country = optional_field(address_editor.country.read(cx).content());
                has_address_content(&address).then_some(address)
            })
            .collect();
        fields.notes = optional_field(editor.notes.read(cx).content());

        let original = editor.original.clone();
        let address_book_id = editor.address_book_id;
        let result = if let Some(mut contact) = original {
            contact.fields = fields;
            self.database
                .as_ref()
                .context("No database is open")
                .and_then(|database| database.update_contact(&contact))
        } else {
            self.database
                .as_ref()
                .context("No database is open")
                .and_then(|database| database.add_contact(address_book_id, &fields).map(|_| ()))
        };

        match result {
            Ok(()) => {
                self.editor = None;
                self.status_message = None;
                self.refresh_contacts(cx);
                self.refresh_dashboard_counts();
            }
            Err(error) => {
                eprintln!("Could not save contact: {error:#}");
                self.set_status(
                    "The contact could not be saved. Check that the database is writable and try again.",
                    true,
                );
            }
        }
    }

    fn refresh_dashboard_counts(&mut self) {
        let Some(database) = self.database.as_ref() else {
            return;
        };
        match (
            database.count_contacts(None),
            database.count_address_books(),
        ) {
            (Ok(contact_count), Ok(address_book_count)) => {
                self.contact_count = contact_count;
                self.address_book_count = address_book_count;
            }
            (Err(error), _) | (_, Err(error)) => {
                eprintln!("Could not refresh dashboard counts: {error:#}");
                self.set_status("Dashboard counts could not be refreshed.", true);
            }
        }
    }

    fn create_default_address_book(&mut self, cx: &mut Context<Self>) {
        let result = self
            .database
            .as_ref()
            .context("No database is open")
            .and_then(|database| database.create_address_book(INITIAL_ADDRESS_BOOK));
        match result {
            Ok(id) => {
                self.address_books = match self
                    .database
                    .as_ref()
                    .context("No database is open")
                    .and_then(Database::list_address_books)
                {
                    Ok(address_books) => address_books,
                    Err(error) => {
                        eprintln!(
                            "Address book was created, but the list could not refresh: {error:#}"
                        );
                        self.set_status(
                            "The address book was created, but the list could not refresh.",
                            true,
                        );
                        return;
                    }
                };
                self.selected_address_book = Some(id);
                self.refresh_dashboard_counts();
                self.refresh_contacts(cx);
            }
            Err(error) => {
                eprintln!("Could not create address book: {error:#}");
                self.set_status(
                    "Could not create the Personal address book. Check that the database is writable.",
                    true,
                );
            }
        }
    }

    fn begin_address_book_creation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let field =
            cx.new(|cx| TextField::new(cx, "new-address-book-name", "Address book name", ""));
        field.read(cx).focus(window);
        self.address_book_name_field = Some(field);
        self.status_message = None;
    }

    fn create_address_book(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.address_book_name_field.as_ref() else {
            return;
        };
        let name = field.read(cx).content().trim().to_owned();
        if let Some(message) = address_book_name_error(&self.address_books, &name) {
            self.set_status(message, true);
            return;
        }

        let result = self
            .database
            .as_ref()
            .context("No database is open")
            .and_then(|database| database.create_address_book(&name));
        match result {
            Ok(id) => {
                self.address_book_name_field = None;
                self.selected_address_book = Some(id);
                self.status_message = None;
                self.refresh_contacts(cx);
                if self.address_books.iter().any(|book| book.id == id) {
                    self.address_book_count = self.address_books.len() as i64;
                    if !self
                        .status_message
                        .as_ref()
                        .is_some_and(|status| status.is_error)
                    {
                        self.set_status(&format!("Address book '{name}' created."), false);
                    }
                } else {
                    self.set_status(
                        "The address book was created, but the list could not be refreshed. Reload the database to see it.",
                        true,
                    );
                }
            }
            Err(error) => {
                eprintln!("Could not create address book: {error:#}");
                self.set_status(
                    "Could not create the address book. Check that the database is writable and try again.",
                    true,
                );
            }
        }
    }

    fn cancel_address_book_creation(&mut self) {
        self.address_book_name_field = None;
        self.status_message = None;
    }

    pub(crate) fn restore_last_database(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.settings.last_database().map(PathBuf::from) {
            self.begin_database_operation(path, false, true, cx);
        }
    }
}
fn optional_field(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn non_empty_labeled_values(
    values: impl IntoIterator<Item = (Option<String>, String)>,
) -> Vec<LabeledValue> {
    values
        .into_iter()
        .filter_map(|(label, value)| {
            optional_field(&value).map(|value| LabeledValue { label, value })
        })
        .collect()
}

fn is_home_address(address: &PostalAddress) -> bool {
    address
        .label
        .as_deref()
        .is_some_and(|label| label.eq_ignore_ascii_case("home"))
}

fn is_work_address(address: &PostalAddress) -> bool {
    address.label.as_deref().is_some_and(|label| {
        label.eq_ignore_ascii_case("work") || label.eq_ignore_ascii_case("business")
    })
}

fn has_address_content(address: &PostalAddress) -> bool {
    [
        address.po_box.as_ref(),
        address.street.as_ref(),
        address.house_number.as_ref(),
        address.extended_address.as_ref(),
        address.city.as_ref(),
        address.region.as_ref(),
        address.postal_code.as_ref(),
        address.country.as_ref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| !value.trim().is_empty())
}

fn address_book_name_error(address_books: &[AddressBook], name: &str) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("Enter a name for the address book.")
    } else if address_books.iter().any(|book| book.name == name) {
        Some("An address book with that name already exists.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeatable_values_preserve_labels_and_ignore_blank_fields() {
        let values = non_empty_labeled_values([
            (Some("Work".into()), " updated@example.com ".into()),
            (Some("Home".into()), "  ".into()),
            (None, "other@example.com".into()),
        ]);

        assert_eq!(values.len(), 2);
        assert_eq!(values[0].label.as_deref(), Some("Work"));
        assert_eq!(values[0].value, "updated@example.com");
        assert_eq!(values[1].label, None);
        assert_eq!(values[1].value, "other@example.com");
    }

    #[test]
    fn blank_optional_fields_are_cleared() {
        assert_eq!(optional_field("  "), None);
        assert_eq!(optional_field(" value "), Some("value".to_owned()));
    }

    #[test]
    fn address_book_names_must_be_nonempty_and_unique() {
        let address_books = vec![AddressBook {
            id: 1,
            name: "Personal".to_owned(),
        }];

        assert_eq!(
            address_book_name_error(&address_books, "  "),
            Some("Enter a name for the address book.")
        );
        assert_eq!(
            address_book_name_error(&address_books, " Personal "),
            Some("An address book with that name already exists.")
        );
        assert_eq!(address_book_name_error(&address_books, "Work"), None);
    }

    #[test]
    fn new_database_starts_with_personal_addressbook() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("addresses.sqlite");

        let prepared = database_workflow::prepare_database(path.clone(), true).unwrap();

        assert_eq!(prepared.path, path);
        assert_eq!(prepared.contact_count, 0);
        assert_eq!(prepared.address_book_count, 1);
        assert_eq!(
            prepared.database.list_address_books().unwrap()[0].name,
            INITIAL_ADDRESS_BOOK
        );
    }

    #[test]
    fn database_filename_requires_supported_extension() {
        assert_eq!(
            database_workflow::normalize_new_database_path(PathBuf::from("contacts"))
                .unwrap()
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("sqlite")
        );
        assert!(
            database_workflow::normalize_new_database_path(PathBuf::from("contacts.txt")).is_err()
        );
    }
}
