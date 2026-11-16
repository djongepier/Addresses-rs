// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
};

use addresses_rs::database::{
    ContactEvent, ContactFields, ContactName, LabeledValue, Organization, PostalAddress,
};
use anyhow::{Context as _, Result, bail};
use csv::StringRecord;
use gpui::{Context, PathPromptOptions};

use super::AddressBookApp;

#[derive(Clone, Copy, Debug)]
pub(super) enum ImportSource {
    Google,
    Outlook,
}

impl ImportSource {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Google => "Google Contacts",
            Self::Outlook => "Outlook",
        }
    }
}

#[derive(Debug)]
pub(super) struct ParsedContactImport {
    pub(super) source: ImportSource,
    pub(super) contacts: Vec<ContactFields>,
    pub(super) skipped_empty_rows: usize,
    pub(super) extra_field_names: Vec<String>,
}

pub(super) struct ContactImportPreview {
    pub(super) file_name: String,
    pub(super) parsed: ParsedContactImport,
    pub(super) target_address_book: Option<i64>,
}

impl AddressBookApp {
    pub(super) fn prompt_import_contacts(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        if self.address_books.is_empty() {
            self.set_status("Create an address book before importing contacts.", true);
            cx.notify();
            return;
        }

        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import contacts from CSV".into()),
        });
        self.is_loading = true;
        self.set_status("Choose a Google or Outlook contacts CSV file.", false);
        cx.notify();

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let file_name = path
                            .file_name()
                            .map_or_else(|| "contacts.csv".to_owned(), |name| name.to_string_lossy().into_owned());
                        let executor = cx.background_executor().clone();
                        let parsed = executor
                            .spawn(async move { parse_contact_csv(&path) })
                            .await;

                        if let Err(error) = this.update(cx, |this, cx| {
                            this.finish_contact_import_preview(parsed, file_name, cx);
                        }) {
                            eprintln!("Could not update the app after reading the contacts CSV: {error:#}");
                        }
                    } else if let Err(error) =
                        this.update(cx, |this, cx| this.cancel_contact_import_selection(cx))
                    {
                        eprintln!("Could not update the app after cancelling contact import: {error:#}");
                    }
                }
                Ok(Ok(None)) => {
                    if let Err(error) =
                        this.update(cx, |this, cx| this.cancel_contact_import_selection(cx))
                    {
                        eprintln!("Could not update the app after cancelling contact import: {error:#}");
                    }
                }
                Ok(Err(error)) => {
                    eprintln!("Could not show the contacts file dialog: {error:#}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        this.set_status(
                            "The file dialog could not be opened. Check your desktop file-dialog support and try again.",
                            true,
                        );
                        cx.notify();
                    }) {
                        eprintln!("Could not report the file-dialog error: {update_error:#}");
                    }
                }
                Err(error) => {
                    eprintln!("The contacts file dialog was interrupted: {error}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        this.set_status(
                            "The file dialog was interrupted. Try importing the contacts again.",
                            true,
                        );
                        cx.notify();
                    }) {
                        eprintln!("Could not report the file-dialog error: {update_error:#}");
                    }
                }
            }
        })
        .detach();
    }

    fn finish_contact_import_preview(
        &mut self,
        parsed: Result<ParsedContactImport>,
        file_name: String,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        match parsed {
            Ok(parsed) => {
                let target_address_book = self
                    .selected_address_book
                    .or_else(|| (self.address_books.len() == 1).then(|| self.address_books[0].id));
                self.import_preview = Some(ContactImportPreview {
                    file_name,
                    parsed,
                    target_address_book,
                });
                self.status_message = None;
            }
            Err(error) => {
                eprintln!("Could not read contacts CSV: {error:#}");
                self.set_status(
                    "The file could not be read as a supported Google or Outlook CSV. Save it as UTF-8 or UTF-16 and try again.",
                    true,
                );
            }
        }
        cx.notify();
    }

    pub(super) fn select_import_address_book(
        &mut self,
        address_book_id: i64,
        cx: &mut Context<Self>,
    ) {
        if self.is_loading {
            return;
        }
        if self
            .address_books
            .iter()
            .any(|address_book| address_book.id == address_book_id)
        {
            if let Some(preview) = self.import_preview.as_mut() {
                preview.target_address_book = Some(address_book_id);
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_contact_import(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        self.import_preview = None;
        self.status_message = None;
        cx.notify();
    }

    fn cancel_contact_import_selection(&mut self, cx: &mut Context<Self>) {
        self.is_loading = false;
        self.status_message = None;
        cx.notify();
    }

    pub(super) fn import_contacts(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }
        let Some(mut preview) = self.import_preview.take() else {
            return;
        };
        let Some(address_book_id) = preview.target_address_book else {
            self.import_preview = Some(preview);
            self.set_status("Choose an address book before importing.", true);
            cx.notify();
            return;
        };
        let Some(database_path) = self.database_path.clone() else {
            self.import_preview = Some(preview);
            self.set_status("Open a database before importing contacts.", true);
            cx.notify();
            return;
        };

        let contacts = std::mem::take(&mut preview.parsed.contacts);
        let executor = cx.background_executor().clone();
        self.is_loading = true;
        self.set_status("Importing contacts…", false);
        cx.notify();

        cx.spawn(async move |this, cx| {
            let (result, contacts) = executor
                .spawn(async move {
                    let result =
                        match addresses_rs::database::Database::open_existing(database_path) {
                            Ok(database) => database.add_contacts(address_book_id, &contacts),
                            Err(error) => Err(error),
                        };
                    (result, contacts)
                })
                .await;

            if let Err(error) = this.update(cx, |this, cx| {
                preview.parsed.contacts = contacts;
                this.finish_contact_import(result, preview, cx);
            }) {
                eprintln!("Could not update the app after importing contacts: {error:#}");
            }
        })
        .detach();
    }

    fn finish_contact_import(
        &mut self,
        result: Result<usize>,
        preview: ContactImportPreview,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        match result {
            Ok(imported_count) => {
                let address_book_name = self
                    .address_books
                    .iter()
                    .find(|book| Some(book.id) == preview.target_address_book)
                    .map_or("the selected address book", |book| book.name.as_str())
                    .to_owned();
                self.import_preview = None;
                self.set_status(
                    &format!(
                        "Imported {imported_count} contacts from {} into {address_book_name}.",
                        preview.file_name
                    ),
                    false,
                );
                self.refresh_dashboard_counts();
                self.refresh_contacts(cx);
            }
            Err(error) => {
                eprintln!("Could not import contacts: {error:#}");
                self.import_preview = Some(preview);
                self.set_status(
                    "The contacts could not be imported. Check that the database is writable and try again.",
                    true,
                );
                cx.notify();
            }
        }
    }
}

fn parse_contact_csv(path: &Path) -> Result<ParsedContactImport> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {
        bail!("The selected file does not have a .csv extension");
    }
    let bytes = fs::read(path).with_context(|| format!("Could not read {}", path.display()))?;
    parse_contact_csv_bytes(&bytes).with_context(|| format!("Could not import {}", path.display()))
}

fn parse_contact_csv_bytes(bytes: &[u8]) -> Result<ParsedContactImport> {
    let contents = decode_csv(bytes)?;
    let delimiter = detect_delimiter(&contents);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(contents.as_bytes());
    let headers = reader
        .headers()
        .context("The CSV file does not contain a valid header row")?
        .clone();
    if headers.is_empty() {
        bail!("The CSV file is empty");
    }

    let source = detect_import_source(&headers)?;
    let mut contacts = Vec::new();
    let mut skipped_empty_rows = 0;
    let mut extra_field_names = BTreeSet::new();
    for (row_index, record) in reader.records().enumerate() {
        let record =
            record.with_context(|| format!("Could not parse CSV row {}", row_index + 2))?;
        if record.iter().all(|value| value.trim().is_empty()) {
            skipped_empty_rows += 1;
            continue;
        }

        let mut columns = CsvColumns::new(&headers);
        let mut fields = match source {
            ImportSource::Google => map_google_contact(&mut columns, &record),
            ImportSource::Outlook => map_outlook_contact(&mut columns, &record),
        };
        columns.preserve_unmapped(&record, &mut fields, &mut extra_field_names);
        contacts.push(fields);
    }

    if contacts.is_empty() {
        bail!("The CSV file contains no non-empty contact rows");
    }

    Ok(ParsedContactImport {
        source,
        contacts,
        skipped_empty_rows,
        extra_field_names: extra_field_names.into_iter().collect(),
    })
}

fn decode_csv(bytes: &[u8]) -> Result<String> {
    if let Some(bytes) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes.to_vec()).context("The CSV file is not valid UTF-8");
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(bytes, true);
    }
    if let Some(bytes) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(bytes, false);
    }
    String::from_utf8(bytes.to_vec()).context("The CSV file is not UTF-8 or UTF-16 encoded")
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        bail!("The UTF-16 CSV file ends with an incomplete character");
    }
    let code_units = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&code_units).context("The UTF-16 CSV file contains invalid characters")
}

fn detect_delimiter(contents: &str) -> u8 {
    let mut counts = [0usize; 3];
    let mut quoted = false;
    let bytes = contents.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' if quoted && bytes.get(index + 1) == Some(&b'"') => index += 1,
            b'"' => quoted = !quoted,
            b'\n' if !quoted => break,
            b',' if !quoted => counts[0] += 1,
            b';' if !quoted => counts[1] += 1,
            b'\t' if !quoted => counts[2] += 1,
            _ => {}
        }
        index += 1;
    }
    [b',', b';', b'\t'][counts
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .map_or(0, |(index, _)| index)]
}

fn detect_import_source(headers: &StringRecord) -> Result<ImportSource> {
    let keys = headers.iter().map(normalize_header).collect::<Vec<_>>();
    let has_google_columns = keys.iter().any(|key| {
        key == "givenname"
            || key == "familyname"
            || key.starts_with("email") && key.ends_with("value")
            || key.starts_with("phone") && key.ends_with("value")
            || key.starts_with("address") && key.ends_with("street")
            || key.starts_with("organization") && key.ends_with("name")
    });
    if has_google_columns {
        return Ok(ImportSource::Google);
    }

    let has_outlook_columns = [
        "firstname",
        "lastname",
        "emailaddress",
        "businessphone",
        "homephone",
    ]
    .iter()
    .any(|expected| keys.iter().any(|key| key == expected));
    if has_outlook_columns {
        return Ok(ImportSource::Outlook);
    }

    bail!("The CSV headers do not match a supported Google Contacts or Outlook export");
}

fn normalize_header(header: &str) -> String {
    header
        .trim_start_matches('\u{feff}')
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

struct CsvColumns {
    raw_headers: Vec<String>,
    normalized_headers: Vec<String>,
    used: HashSet<usize>,
}

impl CsvColumns {
    fn new(headers: &StringRecord) -> Self {
        Self {
            raw_headers: headers.iter().map(str::to_owned).collect(),
            normalized_headers: headers.iter().map(normalize_header).collect(),
            used: HashSet::new(),
        }
    }

    fn take(&mut self, record: &StringRecord, aliases: &[&str]) -> Option<String> {
        let keys = aliases
            .iter()
            .map(|alias| normalize_header(alias))
            .collect::<Vec<_>>();
        self.take_keys(record, &keys)
    }

    fn take_keys(&mut self, record: &StringRecord, keys: &[String]) -> Option<String> {
        keys.iter().find_map(|key| {
            self.normalized_headers
                .iter()
                .position(|header| header == key)
                .and_then(|index| self.take_index(record, index))
        })
    }

    fn take_index(&mut self, record: &StringRecord, index: usize) -> Option<String> {
        self.used.insert(index);
        record
            .get(index)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    fn indexed_groups(&self, prefix: &str) -> BTreeMap<usize, HashMap<String, usize>> {
        let mut groups = BTreeMap::new();
        for (index, key) in self.normalized_headers.iter().enumerate() {
            let Some(remainder) = key.strip_prefix(prefix) else {
                continue;
            };
            let number_length = remainder
                .bytes()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if number_length == 0 {
                continue;
            }
            let Ok(group_index) = remainder[..number_length].parse::<usize>() else {
                continue;
            };
            groups
                .entry(group_index)
                .or_insert_with(HashMap::new)
                .insert(remainder[number_length..].to_owned(), index);
        }
        groups
    }

    fn take_group_value(
        &mut self,
        record: &StringRecord,
        group: &HashMap<String, usize>,
        aliases: &[&str],
    ) -> Option<String> {
        aliases.iter().find_map(|alias| {
            group
                .get(*alias)
                .and_then(|index| self.take_index(record, *index))
        })
    }

    fn take_indexed_values(
        &mut self,
        record: &StringRecord,
        prefix: &str,
        value_field: &str,
        label_fields: &[&str],
    ) -> Vec<LabeledValue> {
        let groups = self.indexed_groups(prefix);
        groups
            .values()
            .filter_map(|group| {
                let value = self.take_group_value(record, group, &[value_field])?;
                let label = self
                    .take_group_value(record, group, label_fields)
                    .map(|label| label.trim_start_matches("* ").to_owned());
                Some(LabeledValue { label, value })
            })
            .collect()
    }

    fn take_indexed_events(&mut self, record: &StringRecord) -> Vec<(String, String)> {
        let groups = self.indexed_groups("event");
        groups
            .values()
            .filter_map(|group| {
                let date = self.take_group_value(record, group, &["value"])?;
                let label = self
                    .take_group_value(record, group, &["type"])
                    .unwrap_or_default();
                Some((label, date))
            })
            .collect()
    }

    fn take_google_addresses(&mut self, record: &StringRecord) -> Vec<PostalAddress> {
        let groups = self.indexed_groups("address");
        groups
            .values()
            .filter_map(|group| {
                let mut address = PostalAddress {
                    label: self.take_group_value(record, group, &["type"]),
                    po_box: self.take_group_value(record, group, &["pobox"]),
                    street: self
                        .take_group_value(record, group, &["street"])
                        .or_else(|| self.take_group_value(record, group, &["formatted"])),
                    house_number: self.take_group_value(record, group, &["housenumber"]),
                    extended_address: self.take_group_value(record, group, &["extendedaddress"]),
                    city: self.take_group_value(record, group, &["city"]),
                    region: self.take_group_value(record, group, &["region"]),
                    postal_code: self.take_group_value(record, group, &["postalcode"]),
                    country: self.take_group_value(record, group, &["country"]),
                };
                if address_is_empty(&address) {
                    None
                } else {
                    Some(std::mem::take(&mut address))
                }
            })
            .collect()
    }

    fn take_prefixed(
        &mut self,
        record: &StringRecord,
        prefix: &str,
        suffixes: &[&str],
    ) -> Option<String> {
        let keys = suffixes
            .iter()
            .map(|suffix| format!("{prefix}{suffix}"))
            .collect::<Vec<_>>();
        self.take_keys(record, &keys)
    }

    fn take_outlook_address(
        &mut self,
        record: &StringRecord,
        prefix: &str,
        label: &str,
    ) -> Option<PostalAddress> {
        let mut street_parts = [
            self.take_prefixed(record, prefix, &["street"]),
            self.take_prefixed(record, prefix, &["street2"]),
            self.take_prefixed(record, prefix, &["street3"]),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        let full_address = self.take_prefixed(record, prefix, &["address"]);
        if street_parts.is_empty() {
            street_parts.extend(full_address);
        }
        let address = PostalAddress {
            label: Some(label.to_owned()),
            po_box: self.take_prefixed(record, prefix, &["pobox"]),
            street: (!street_parts.is_empty()).then(|| street_parts.join("\n")),
            house_number: self.take_prefixed(record, prefix, &["housenumber", "streetnumber"]),
            extended_address: self.take_prefixed(record, prefix, &["extendedaddress"]),
            city: self.take_prefixed(record, prefix, &["city"]),
            region: self.take_prefixed(record, prefix, &["state", "province", "region"]),
            postal_code: self.take_prefixed(record, prefix, &["postalcode", "zipcode"]),
            country: self.take_prefixed(record, prefix, &["countryregion", "country"]),
        };
        (!address_is_empty(&address)).then_some(address)
    }

    fn preserve_unmapped(
        &mut self,
        record: &StringRecord,
        fields: &mut ContactFields,
        extra_field_names: &mut BTreeSet<String>,
    ) {
        let mut used_names = HashMap::<String, usize>::new();
        for index in 0..self.normalized_headers.len() {
            if self.used.contains(&index) {
                continue;
            }
            let Some(value) = record
                .get(index)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            let header = self.raw_headers[index].trim();
            let base_name = if header.is_empty() {
                format!("Column {}", index + 1)
            } else {
                header.to_owned()
            };
            let count = used_names.entry(base_name.clone()).or_default();
            *count += 1;
            let name = if *count == 1 {
                base_name
            } else {
                format!("{base_name} [{}]", count)
            };
            fields
                .extra_fields
                .insert(name.clone(), serde_json::Value::String(value.to_owned()));
            extra_field_names.insert(name);
        }
    }
}

fn map_google_contact(columns: &mut CsvColumns, record: &StringRecord) -> ContactFields {
    let mut fields = ContactFields {
        name: ContactName {
            display_name: columns.take(record, &["Name", "Display Name"]),
            prefix: columns.take(record, &["Name Prefix"]),
            given_name: columns.take(record, &["Given Name", "First Name"]),
            middle_name: columns.take(record, &["Additional Name", "Middle Name"]),
            family_name: columns.take(record, &["Family Name", "Last Name"]),
            suffix: columns.take(record, &["Name Suffix"]),
            initials: columns.take(record, &["Initials"]),
            nickname: columns.take(record, &["Nickname"]),
            file_as: columns.take(record, &["File As"]),
            phonetic_given_name: columns.take(record, &["Given Name Yomi", "Phonetic First Name"]),
            phonetic_middle_name: columns
                .take(record, &["Additional Name Yomi", "Phonetic Middle Name"]),
            phonetic_family_name: columns.take(record, &["Family Name Yomi", "Phonetic Last Name"]),
        },
        organization: Organization {
            company: columns.take(record, &["Organization 1 - Name", "Company"]),
            department: columns.take(record, &["Organization 1 - Department"]),
            job_title: columns.take(record, &["Organization 1 - Title"]),
        },
        notes: columns.take(record, &["Notes"]),
        birthday: columns.take(record, &["Birthday"]),
        anniversary: columns.take(record, &["Anniversary"]),
        emails: columns.take_indexed_values(record, "email", "value", &["type"]),
        phones: columns.take_indexed_values(record, "phone", "value", &["type"]),
        websites: columns.take_indexed_values(record, "website", "value", &["type"]),
        instant_messages: columns.take_indexed_values(record, "im", "value", &["service", "type"]),
        relationships: columns.take_indexed_values(record, "relation", "value", &["type"]),
        addresses: columns.take_google_addresses(record),
        ..ContactFields::default()
    };

    for (label, date) in columns.take_indexed_events(record) {
        match label.to_ascii_lowercase().as_str() {
            "birthday" if fields.birthday.is_none() => fields.birthday = Some(date),
            "anniversary" if fields.anniversary.is_none() => fields.anniversary = Some(date),
            _ => fields.other_events.push(ContactEvent {
                label: (!label.is_empty()).then_some(label),
                date,
            }),
        }
    }
    if let Some(groups) = columns.take(record, &["Group Membership"]) {
        fields.categories = groups
            .split(":::")
            .map(str::trim)
            .map(|group| group.trim_start_matches("* ").to_owned())
            .filter(|group| !group.is_empty() && group != "myContacts")
            .collect();
    }
    fields
}

fn map_outlook_contact(columns: &mut CsvColumns, record: &StringRecord) -> ContactFields {
    let mut fields = ContactFields {
        name: ContactName {
            display_name: columns.take(record, &["Display Name", "Full Name"]),
            prefix: columns.take(record, &["Title"]),
            given_name: columns.take(record, &["First Name"]),
            middle_name: columns.take(record, &["Middle Name"]),
            family_name: columns.take(record, &["Last Name"]),
            suffix: columns.take(record, &["Suffix"]),
            initials: columns.take(record, &["Initials"]),
            nickname: columns.take(record, &["Nickname"]),
            file_as: columns.take(record, &["File As"]),
            ..ContactName::default()
        },
        organization: Organization {
            company: columns.take(record, &["Company"]),
            department: columns.take(record, &["Department"]),
            job_title: columns.take(record, &["Job Title"]),
        },
        notes: columns.take(record, &["Notes"]),
        birthday: columns.take(record, &["Birthday"]),
        anniversary: columns.take(record, &["Anniversary"]),
        ..ContactFields::default()
    };

    for (aliases, label) in [
        (&["emailaddress", "email1address"][..], None),
        (&["email2address"][..], Some("Other")),
        (&["email3address"][..], Some("Other")),
    ] {
        if let Some(value) = columns.take(record, aliases) {
            fields.emails.push(LabeledValue {
                label: label.map(str::to_owned),
                value,
            });
        }
    }
    for (aliases, label) in [
        (&["homephone"][..], Some("Home")),
        (&["homephone2"][..], Some("Home")),
        (&["businessphone"][..], Some("Work")),
        (&["businessphone2"][..], Some("Work")),
        (&["mobilephone"][..], Some("Mobile")),
        (&["primaryphone"][..], Some("Primary")),
        (&["otherphone"][..], Some("Other")),
        (&["businessfax"][..], Some("Work fax")),
        (&["homefax"][..], Some("Home fax")),
        (&["pager"][..], Some("Pager")),
    ] {
        if let Some(value) = columns.take(record, aliases) {
            fields.phones.push(LabeledValue {
                label: label.map(str::to_owned),
                value,
            });
        }
    }
    for (aliases, label) in [(&["webpage"][..], None), (&["webpage2"][..], Some("Other"))] {
        if let Some(value) = columns.take(record, aliases) {
            fields.websites.push(LabeledValue {
                label: label.map(str::to_owned),
                value,
            });
        }
    }

    for (prefix, label) in [("business", "Work"), ("home", "Home"), ("other", "Other")] {
        if let Some(address) = columns.take_outlook_address(record, prefix, label) {
            fields.addresses.push(address);
        }
    }
    for (header, label) in [
        (&["spouse"][..], "Spouse"),
        (&["manager"][..], "Manager"),
        (&["assistant"][..], "Assistant"),
    ] {
        if let Some(value) = columns.take(record, header) {
            fields.relationships.push(LabeledValue {
                label: Some(label.to_owned()),
                value,
            });
        }
    }
    if let Some(address) = columns.take(record, &["im Address", "Instant Messaging Address"]) {
        fields.instant_messages.push(LabeledValue {
            label: None,
            value: address,
        });
    }
    if let Some(categories) = columns.take(record, &["Categories"]) {
        fields.categories = categories
            .split(';')
            .map(str::trim)
            .filter(|category| !category.is_empty())
            .map(str::to_owned)
            .collect();
    }
    fields
}

fn address_is_empty(address: &PostalAddress) -> bool {
    address.po_box.is_none()
        && address.street.is_none()
        && address.house_number.is_none()
        && address.extended_address.is_none()
        && address.city.is_none()
        && address.region.is_none()
        && address.postal_code.is_none()
        && address.country.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_google_contacts_and_preserves_unmapped_columns() {
        let csv = concat!(
            "Name,Given Name,Family Name,E-mail 1 - Type,E-mail 1 - Value,",
            "E-mail 2 - Type,E-mail 2 - Value,Phone 1 - Type,Phone 1 - Value,",
            "Address 1 - Street,Address 1 - House Number,Address 1 - Postal Code,",
            "Address 1 - City,Custom Column\n",
            "\"Ada Lovelace\",Ada,Lovelace,Work,ada@example.com,Home,",
            "ada@home.example,Mobile,+441234,Engine Road,42,12345,London,",
            "\"kept as extra\"\n"
        );
        let parsed = parse_contact_csv_bytes(csv.as_bytes()).unwrap();

        assert!(matches!(parsed.source, ImportSource::Google));
        assert_eq!(parsed.contacts.len(), 1);
        let contact = &parsed.contacts[0];
        assert_eq!(contact.display_name(), "Ada Lovelace");
        assert_eq!(contact.emails.len(), 2);
        assert_eq!(contact.emails[1].label.as_deref(), Some("Home"));
        assert_eq!(contact.addresses[0].street.as_deref(), Some("Engine Road"));
        assert_eq!(contact.addresses[0].house_number.as_deref(), Some("42"));
        assert_eq!(contact.addresses[0].postal_code.as_deref(), Some("12345"));
        assert_eq!(contact.addresses[0].city.as_deref(), Some("London"));
        assert_eq!(
            contact.extra_fields["Custom Column"],
            serde_json::Value::String("kept as extra".to_owned())
        );
        assert_eq!(parsed.extra_field_names, vec!["Custom Column".to_owned()]);
    }

    #[test]
    fn imports_outlook_contacts_from_semicolon_delimited_utf16_csv() {
        let text = concat!(
            "First Name;Last Name;E-mail Address;E-mail 2 Address;Mobile Phone;",
            "Business Street;Business Street Number;Business Postal Code;",
            "Business City;Business Country/Region;Categories;Notes\n",
            "Grace;Hopper;grace@example.com;grace@navy.example;+12025550123;",
            "Navy Way;1;22203;Arlington;United States;Navy;Compiler pioneer\n"
        );
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        let parsed = parse_contact_csv_bytes(&bytes).unwrap();

        assert!(matches!(parsed.source, ImportSource::Outlook));
        let contact = &parsed.contacts[0];
        assert_eq!(contact.display_name(), "Grace Hopper");
        assert_eq!(contact.emails.len(), 2);
        assert_eq!(contact.phones[0].label.as_deref(), Some("Mobile"));
        assert_eq!(contact.addresses[0].street.as_deref(), Some("Navy Way"));
        assert_eq!(contact.addresses[0].house_number.as_deref(), Some("1"));
        assert_eq!(contact.addresses[0].postal_code.as_deref(), Some("22203"));
        assert_eq!(
            contact.addresses[0].country.as_deref(),
            Some("United States")
        );
        assert_eq!(contact.categories, vec!["Navy".to_owned()]);
        assert_eq!(contact.notes.as_deref(), Some("Compiler pioneer"));
    }

    #[test]
    fn imports_google_contacts_with_first_middle_last_name_headers() {
        let csv = concat!(
            "First Name\tMiddle Name\tLast Name\tPhonetic First Name\t",
            "Phonetic Middle Name\tPhonetic Last Name\tName Prefix\tName Suffix\t",
            "Address 1 - Street\n",
            "Ada\tByron\tLovelace\tAY-dah\tBY-ron\tLUHV-lays\tCountess\tJr.\t",
            "1 Engine Road\n"
        );
        let parsed = parse_contact_csv_bytes(csv.as_bytes()).unwrap();

        assert!(matches!(parsed.source, ImportSource::Google));
        let contact = &parsed.contacts[0];
        assert_eq!(contact.name.given_name.as_deref(), Some("Ada"));
        assert_eq!(contact.name.middle_name.as_deref(), Some("Byron"));
        assert_eq!(contact.name.family_name.as_deref(), Some("Lovelace"));
        assert_eq!(contact.name.phonetic_given_name.as_deref(), Some("AY-dah"));
        assert_eq!(contact.name.phonetic_middle_name.as_deref(), Some("BY-ron"));
        assert_eq!(
            contact.name.phonetic_family_name.as_deref(),
            Some("LUHV-lays")
        );
        assert_eq!(contact.display_name(), "Countess Ada Byron Lovelace Jr.");
    }

    #[test]
    fn rejects_unknown_csv_formats_and_skips_blank_rows() {
        assert!(
            parse_contact_csv_bytes(b"Product,Quantity\nBook,2\n")
                .unwrap_err()
                .to_string()
                .contains("do not match a supported")
        );

        let parsed =
            parse_contact_csv_bytes(b"First Name,Last Name\n,,\nAda,Lovelace\n,,\n").unwrap();
        assert_eq!(parsed.contacts.len(), 1);
        assert_eq!(parsed.skipped_empty_rows, 2);
    }
}
