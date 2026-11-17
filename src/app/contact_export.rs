// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::{
    collections::{BTreeSet, HashMap},
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use addresses_rs::database::ContactFields;
use anyhow::{Context as _, Result, bail};
use gpui::Context;
use serde::Serialize;
use serde_json::Value;

use super::AddressBookApp;

#[derive(Clone, Copy)]
pub(super) enum ExportFormat {
    Csv,
    Json,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Json => "JSON",
        }
    }
}

struct ExportContact {
    address_book: String,
    fields: ContactFields,
}

#[derive(Serialize)]
struct JsonExportContact<'a> {
    address_book: &'a str,
    fields: &'a ContactFields,
}

impl AddressBookApp {
    pub(super) fn prompt_export_contacts(&mut self, format: ExportFormat, cx: &mut Context<Self>) {
        if self.is_loading || self.selected_contact_ids.is_empty() {
            return;
        }

        let Some(database_path) = self.database_path.as_deref() else {
            self.set_status("Open a database before exporting contacts.", true);
            cx.notify();
            return;
        };
        let directory = database_path.parent().unwrap_or_else(|| Path::new("."));
        let receiver =
            cx.prompt_for_new_path(directory, Some(&format!("contacts.{}", format.extension())));
        let contacts = self
            .contacts
            .iter()
            .filter(|contact| self.selected_contact_ids.contains(&contact.id))
            .map(|contact| ExportContact {
                address_book: self
                    .address_books
                    .iter()
                    .find(|book| book.id == contact.address_book_id)
                    .map_or_else(
                        || "Unknown address book".to_owned(),
                        |book| book.name.clone(),
                    ),
                fields: contact.fields.clone(),
            })
            .collect::<Vec<_>>();

        self.is_loading = true;
        self.set_status(
            &format!(
                "Choose where to save the selected contacts as {}.",
                format.label()
            ),
            false,
        );
        cx.notify();

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(path))) => {
                    let executor = cx.background_executor().clone();
                    let result = executor
                        .spawn(async move { write_export(&path, format, &contacts) })
                        .await;
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.finish_contact_export(result, format, cx);
                    }) {
                        eprintln!("Could not update the app after exporting contacts: {error:#}");
                    }
                }
                Ok(Ok(None)) => {
                    if let Err(error) = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        this.status_message = None;
                        cx.notify();
                    }) {
                        eprintln!("Could not update the app after cancelling contact export: {error:#}");
                    }
                }
                Ok(Err(error)) => {
                    eprintln!("Could not show the contacts save dialog: {error:#}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        this.set_status(
                            "The save dialog could not be opened. Check your desktop file-dialog support and try again.",
                            true,
                        );
                        cx.notify();
                    }) {
                        eprintln!("Could not report the save-dialog error: {update_error:#}");
                    }
                }
                Err(error) => {
                    eprintln!("The contacts save dialog was interrupted: {error}");
                    if let Err(update_error) = this.update(cx, |this, cx| {
                        this.is_loading = false;
                        this.set_status(
                            "The save dialog was interrupted. Try exporting the contacts again.",
                            true,
                        );
                        cx.notify();
                    }) {
                        eprintln!("Could not report the save-dialog error: {update_error:#}");
                    }
                }
            }
        })
        .detach();
    }

    fn finish_contact_export(
        &mut self,
        result: Result<usize>,
        format: ExportFormat,
        cx: &mut Context<Self>,
    ) {
        self.is_loading = false;
        match result {
            Ok(count) => self.set_status(
                &format!("Exported {count} contacts as {}.", format.label()),
                false,
            ),
            Err(error) => {
                eprintln!("Could not export contacts: {error:#}");
                self.set_status(
                    &format!(
                        "The contacts could not be exported. Choose a filename ending in .{} and check that the destination is writable.",
                        format.extension()
                    ),
                    true,
                );
            }
        }
        cx.notify();
    }
}

fn write_export(path: &Path, format: ExportFormat, contacts: &[ExportContact]) -> Result<usize> {
    let extension = path.extension().and_then(|value| value.to_str());
    if !extension.is_some_and(|extension| extension.eq_ignore_ascii_case(format.extension())) {
        bail!("The selected filename must end in .{}", format.extension());
    }

    let file = File::create(path)
        .with_context(|| format!("Could not create export file at {}", path.display()))?;
    let writer = BufWriter::new(file);
    match format {
        ExportFormat::Csv => write_csv(writer, contacts)?,
        ExportFormat::Json => write_json(writer, contacts)?,
    }
    Ok(contacts.len())
}

fn write_json(mut writer: impl Write, contacts: &[ExportContact]) -> Result<()> {
    let entries = contacts
        .iter()
        .map(|contact| JsonExportContact {
            address_book: &contact.address_book,
            fields: &contact.fields,
        })
        .collect::<Vec<_>>();
    serde_json::to_writer_pretty(&mut writer, &entries)
        .context("Could not encode contacts as JSON")?;
    writer
        .flush()
        .context("Could not finish writing the JSON file")
}

fn write_csv(writer: impl Write, contacts: &[ExportContact]) -> Result<()> {
    let mut headers = vec![
        "Name".to_owned(),
        "Name Prefix".to_owned(),
        "Given Name".to_owned(),
        "Additional Name".to_owned(),
        "Family Name".to_owned(),
        "Name Suffix".to_owned(),
        "Initials".to_owned(),
        "Nickname".to_owned(),
        "File As".to_owned(),
        "Given Name Yomi".to_owned(),
        "Additional Name Yomi".to_owned(),
        "Family Name Yomi".to_owned(),
        "Organization 1 - Name".to_owned(),
        "Organization 1 - Department".to_owned(),
        "Organization 1 - Title".to_owned(),
        "Birthday".to_owned(),
        "Anniversary".to_owned(),
        "Notes".to_owned(),
        "Group Membership".to_owned(),
        "Address book".to_owned(),
    ];
    let max_emails = contacts
        .iter()
        .map(|contact| contact.fields.emails.len())
        .max()
        .unwrap_or_default();
    let max_phones = contacts
        .iter()
        .map(|contact| contact.fields.phones.len())
        .max()
        .unwrap_or_default();
    let max_addresses = contacts
        .iter()
        .map(|contact| contact.fields.addresses.len())
        .max()
        .unwrap_or_default();
    let max_websites = contacts
        .iter()
        .map(|contact| contact.fields.websites.len())
        .max()
        .unwrap_or_default();
    let max_messages = contacts
        .iter()
        .map(|contact| contact.fields.instant_messages.len())
        .max()
        .unwrap_or_default();
    let max_relationships = contacts
        .iter()
        .map(|contact| contact.fields.relationships.len())
        .max()
        .unwrap_or_default();
    let max_events = contacts
        .iter()
        .map(|contact| contact.fields.other_events.len())
        .max()
        .unwrap_or_default();
    for (group, count, columns) in [
        ("E-mail", max_emails, &["Type", "Value"][..]),
        ("Phone", max_phones, &["Type", "Value"][..]),
        (
            "Address",
            max_addresses,
            &[
                "Type",
                "PO Box",
                "Street",
                "House Number",
                "Extended Address",
                "City",
                "Region",
                "Postal Code",
                "Country",
            ][..],
        ),
        ("Website", max_websites, &["Type", "Value"][..]),
        ("IM", max_messages, &["Service", "Value"][..]),
        ("Relation", max_relationships, &["Type", "Value"][..]),
        ("Event", max_events, &["Type", "Value"][..]),
    ] {
        for index in 1..=count {
            headers.extend(
                columns
                    .iter()
                    .map(|column| format!("{group} {index} - {column}")),
            );
        }
    }

    let extra_names = contacts
        .iter()
        .flat_map(|contact| contact.fields.extra_fields.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut occupied_headers = headers.iter().cloned().collect::<BTreeSet<_>>();
    let mut extra_header_names = HashMap::new();
    for name in extra_names {
        let mut header = if occupied_headers.contains(&name) {
            format!("Extra: {name}")
        } else {
            name.clone()
        };
        let base_header = header.clone();
        let mut suffix = 2;
        while !occupied_headers.insert(header.clone()) {
            header = format!("{base_header} [{suffix}]");
            suffix += 1;
        }
        extra_header_names.insert(name, header);
    }
    headers.extend(
        extra_header_names
            .values()
            .cloned()
            .collect::<BTreeSet<_>>(),
    );
    let header_indices = headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.as_str(), index))
        .collect::<HashMap<_, _>>();

    let mut csv = csv::Writer::from_writer(writer);
    csv.write_record(&headers)
        .context("Could not write CSV headers")?;
    for contact in contacts {
        let fields = &contact.fields;
        let mut row = vec![String::new(); headers.len()];
        set_cell(
            &mut row,
            &header_indices,
            "Name",
            Some(&fields.display_name()),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Name Prefix",
            fields.name.prefix.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Given Name",
            fields.name.given_name.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Additional Name",
            fields.name.middle_name.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Family Name",
            fields.name.family_name.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Name Suffix",
            fields.name.suffix.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Initials",
            fields.name.initials.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Nickname",
            fields.name.nickname.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "File As",
            fields.name.file_as.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Given Name Yomi",
            fields.name.phonetic_given_name.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Additional Name Yomi",
            fields.name.phonetic_middle_name.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Family Name Yomi",
            fields.name.phonetic_family_name.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Organization 1 - Name",
            fields.organization.company.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Organization 1 - Department",
            fields.organization.department.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Organization 1 - Title",
            fields.organization.job_title.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Birthday",
            fields.birthday.as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Anniversary",
            fields.anniversary.as_deref(),
        );
        set_cell(&mut row, &header_indices, "Notes", fields.notes.as_deref());
        set_cell(
            &mut row,
            &header_indices,
            "Group Membership",
            (!fields.categories.is_empty())
                .then(|| fields.categories.join(":::"))
                .as_deref(),
        );
        set_cell(
            &mut row,
            &header_indices,
            "Address book",
            Some(&contact.address_book),
        );
        for (index, value) in fields.emails.iter().enumerate() {
            set_group_cell(
                &mut row,
                &header_indices,
                "E-mail",
                index,
                "Type",
                value.label.as_deref(),
            );
            set_group_cell(
                &mut row,
                &header_indices,
                "E-mail",
                index,
                "Value",
                Some(&value.value),
            );
        }
        for (index, value) in fields.phones.iter().enumerate() {
            set_group_cell(
                &mut row,
                &header_indices,
                "Phone",
                index,
                "Type",
                value.label.as_deref(),
            );
            set_group_cell(
                &mut row,
                &header_indices,
                "Phone",
                index,
                "Value",
                Some(&value.value),
            );
        }
        for (index, address) in fields.addresses.iter().enumerate() {
            for (column, value) in [
                ("Type", address.label.as_deref()),
                ("PO Box", address.po_box.as_deref()),
                ("Street", address.street.as_deref()),
                ("House Number", address.house_number.as_deref()),
                ("Extended Address", address.extended_address.as_deref()),
                ("City", address.city.as_deref()),
                ("Region", address.region.as_deref()),
                ("Postal Code", address.postal_code.as_deref()),
                ("Country", address.country.as_deref()),
            ] {
                set_group_cell(&mut row, &header_indices, "Address", index, column, value);
            }
        }
        for (index, value) in fields.websites.iter().enumerate() {
            set_group_cell(
                &mut row,
                &header_indices,
                "Website",
                index,
                "Type",
                value.label.as_deref(),
            );
            set_group_cell(
                &mut row,
                &header_indices,
                "Website",
                index,
                "Value",
                Some(&value.value),
            );
        }
        for (index, value) in fields.instant_messages.iter().enumerate() {
            set_group_cell(
                &mut row,
                &header_indices,
                "IM",
                index,
                "Service",
                value.label.as_deref(),
            );
            set_group_cell(
                &mut row,
                &header_indices,
                "IM",
                index,
                "Value",
                Some(&value.value),
            );
        }
        for (index, value) in fields.relationships.iter().enumerate() {
            set_group_cell(
                &mut row,
                &header_indices,
                "Relation",
                index,
                "Type",
                value.label.as_deref(),
            );
            set_group_cell(
                &mut row,
                &header_indices,
                "Relation",
                index,
                "Value",
                Some(&value.value),
            );
        }
        for (index, event) in fields.other_events.iter().enumerate() {
            set_group_cell(
                &mut row,
                &header_indices,
                "Event",
                index,
                "Type",
                event.label.as_deref(),
            );
            set_group_cell(
                &mut row,
                &header_indices,
                "Event",
                index,
                "Value",
                Some(&event.date),
            );
        }
        for (name, value) in &fields.extra_fields {
            let header = extra_header_names
                .get(name)
                .expect("all extra fields have an export column");
            let value = match value {
                Value::String(value) => value.clone(),
                value => value.to_string(),
            };
            set_cell(&mut row, &header_indices, header, Some(&value));
        }
        csv.write_record(row)
            .context("Could not write a contact row")?;
    }
    csv.flush().context("Could not finish writing the CSV file")
}

fn set_group_cell(
    row: &mut [String],
    header_indices: &HashMap<&str, usize>,
    group: &str,
    index: usize,
    column: &str,
    value: Option<&str>,
) {
    let header = format!("{group} {} - {column}", index + 1);
    set_cell(row, header_indices, &header, value);
}

fn set_cell(
    row: &mut [String],
    header_indices: &HashMap<&str, usize>,
    header: &str,
    value: Option<&str>,
) {
    if let (Some(index), Some(value)) = (header_indices.get(header), value) {
        row[*index] = value.to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use addresses_rs::database::{LabeledValue, PostalAddress};

    #[test]
    fn csv_export_writes_repeatable_fields_and_extra_columns() {
        let contact = ExportContact {
            address_book: "Family".to_owned(),
            fields: ContactFields {
                name: addresses_rs::database::ContactName {
                    given_name: Some("Ada".to_owned()),
                    family_name: Some("Lovelace".to_owned()),
                    ..Default::default()
                },
                emails: vec![LabeledValue {
                    label: Some("Home".to_owned()),
                    value: "ada@example.com".to_owned(),
                }],
                addresses: vec![PostalAddress {
                    street: Some("Engine Road".to_owned()),
                    house_number: Some("42".to_owned()),
                    ..Default::default()
                }],
                extra_fields: [
                    (
                        "Custom Column".to_owned(),
                        Value::String("preserved".to_owned()),
                    ),
                    ("Name".to_owned(), Value::String("custom name".to_owned())),
                    (
                        "Extra: Name".to_owned(),
                        Value::String("another custom value".to_owned()),
                    ),
                ]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        };
        let mut output = Vec::new();
        write_csv(&mut output, &[contact]).unwrap();
        let mut reader = csv::Reader::from_reader(output.as_slice());
        let headers = reader.headers().unwrap().clone();
        let row = reader.records().next().unwrap().unwrap();

        assert_eq!(
            row.get(headers.iter().position(|name| name == "Name").unwrap()),
            Some("Ada Lovelace")
        );
        assert_eq!(
            row.get(
                headers
                    .iter()
                    .position(|name| name == "E-mail 1 - Value")
                    .unwrap()
            ),
            Some("ada@example.com")
        );
        assert_eq!(
            row.get(
                headers
                    .iter()
                    .position(|name| name == "Address 1 - House Number")
                    .unwrap()
            ),
            Some("42")
        );
        assert_eq!(
            row.get(
                headers
                    .iter()
                    .position(|name| name == "Custom Column")
                    .unwrap()
            ),
            Some("preserved")
        );
        assert_eq!(
            row.get(
                headers
                    .iter()
                    .position(|name| name == "Extra: Name")
                    .unwrap()
            ),
            Some("another custom value")
        );
        assert_eq!(
            row.get(
                headers
                    .iter()
                    .position(|name| name == "Extra: Name [2]")
                    .unwrap()
            ),
            Some("custom name")
        );
        assert_eq!(headers.iter().collect::<BTreeSet<_>>().len(), headers.len());
        let parsed = super::super::contact_import::parse_contact_csv_bytes(&output).unwrap();
        assert_eq!(parsed.contacts[0].display_name(), "Ada Lovelace");
        assert_eq!(parsed.contacts[0].emails[0].value, "ada@example.com");
        assert_eq!(
            parsed.contacts[0].extra_fields["Custom Column"],
            Value::String("preserved".to_owned())
        );
    }

    #[test]
    fn json_export_includes_address_book_and_full_fields() {
        let contact = ExportContact {
            address_book: "Family".to_owned(),
            fields: ContactFields {
                notes: Some("A note".to_owned()),
                ..Default::default()
            },
        };
        let mut output = Vec::new();
        write_json(&mut output, &[contact]).unwrap();
        let json: Value = serde_json::from_slice(&output).unwrap();

        assert_eq!(json[0]["address_book"], "Family");
        assert_eq!(json[0]["fields"]["notes"], "A note");
    }

    #[test]
    fn export_requires_the_selected_format_extension() {
        assert!(write_export(Path::new("contacts.json"), ExportFormat::Csv, &[]).is_err());
    }
}
