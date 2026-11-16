// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::{fs, fs::OpenOptions, path::Path};

use anyhow::{Context, Result, bail};
use rusqlite::types::Type;
use rusqlite::{Connection, Error, OpenFlags, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A named collection of contacts stored in the same database file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressBook {
    pub id: i64,
    pub name: String,
}

/// A contact record with the shared Google and Outlook fields grouped by purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub id: i64,
    pub address_book_id: i64,
    pub fields: ContactFields,
}

/// Shared contact data; unrecognized provider columns stay in `extra_fields`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactFields {
    pub name: ContactName,
    pub organization: Organization,
    pub emails: Vec<LabeledValue>,
    pub phones: Vec<LabeledValue>,
    pub addresses: Vec<PostalAddress>,
    pub websites: Vec<LabeledValue>,
    pub instant_messages: Vec<LabeledValue>,
    pub birthday: Option<String>,
    pub anniversary: Option<String>,
    pub other_events: Vec<ContactEvent>,
    pub relationships: Vec<LabeledValue>,
    pub notes: Option<String>,
    pub categories: Vec<String>,
    pub extra_fields: Map<String, Value>,
}

/// Name parts support structured exports as well as a readable display name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactName {
    pub display_name: Option<String>,
    pub prefix: Option<String>,
    pub given_name: Option<String>,
    pub middle_name: Option<String>,
    pub family_name: Option<String>,
    pub suffix: Option<String>,
    pub initials: Option<String>,
    pub nickname: Option<String>,
    pub file_as: Option<String>,
    pub phonetic_given_name: Option<String>,
    pub phonetic_middle_name: Option<String>,
    pub phonetic_family_name: Option<String>,
}

/// Organization properties common to contact exports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Organization {
    pub company: Option<String>,
    pub department: Option<String>,
    pub job_title: Option<String>,
}

/// A repeatable contact value retains its provider label, such as Home or Work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabeledValue {
    pub label: Option<String>,
    pub value: String,
}

/// A postal address keeps its label and components separate for import/export.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostalAddress {
    pub label: Option<String>,
    pub po_box: Option<String>,
    pub street: Option<String>,
    pub house_number: Option<String>,
    pub extended_address: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
}

/// Events beyond the dedicated birthday and anniversary fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactEvent {
    pub label: Option<String>,
    pub date: String,
}

/// Owns the SQLite connection and provides the app's small persistence API.
pub struct Database {
    connection: Connection,
}

impl Database {
    /// Opens or creates a database file and initializes its schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let connection = Connection::open(path)
            .with_context(|| format!("Could not open database at {}", path.display()))?;
        Self::from_connection(connection)
            .with_context(|| format!("Could not initialize database at {}", path.display()))
    }

    /// Creates a new database without overwriting an existing file.
    pub fn create_new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let reservation = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("Could not create database file at {}", path.display()))?;
        drop(reservation);

        let connection = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        {
            Ok(connection) => connection,
            Err(error) => {
                return Err(
                    cleanup_incomplete_database(path, error.into()).context(format!(
                        "Could not initialize database at {}",
                        path.display()
                    )),
                );
            }
        };

        let result = Self::from_connection(connection)
            .with_context(|| format!("Could not create database at {}", path.display()));
        match result {
            Ok(database) => Ok(database),
            Err(error) => Err(cleanup_incomplete_database(path, error)),
        }
    }

    /// Opens an existing Addresses database without creating missing files or tables.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .with_context(|| format!("Could not open existing database at {}", path.display()))?;
        Self::validate_schema(&connection).with_context(|| {
            format!("Could not open {} as an Addresses database", path.display())
        })?;
        Self::from_connection(connection)
            .with_context(|| format!("Could not initialize database at {}", path.display()))
    }

    /// Creates an in-memory database for tests and temporary use.
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
            .context("Could not initialize in-memory database")
    }

    fn validate_schema(connection: &Connection) -> Result<()> {
        let table_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN ('address_books', 'contacts')",
                [],
                |row| row.get(0),
            )
            .context("This file is not a readable SQLite database")?;
        if table_count != 2 {
            bail!("This file does not contain an Addresses database");
        }

        connection
            .prepare("SELECT id, name FROM address_books LIMIT 0")
            .context("The address-book table has an unsupported structure")?;
        connection
            .prepare(
                "SELECT id, address_book_id, display_name, fields_json
                 FROM contacts LIMIT 0",
            )
            .context("The contacts table has an unsupported structure")?;
        Ok(())
    }

    /// Enables referential integrity and creates the initial tables and indexes.
    fn from_connection(connection: Connection) -> Result<Self> {
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS address_books (
                    id   INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE CHECK (length(trim(name)) > 0)
                );

                CREATE TABLE IF NOT EXISTS contacts (
                    id              INTEGER PRIMARY KEY,
                    address_book_id INTEGER NOT NULL
                        REFERENCES address_books(id) ON DELETE CASCADE,
                    display_name    TEXT NOT NULL DEFAULT '',
                    fields_json     TEXT NOT NULL DEFAULT '{}'
                );

                CREATE INDEX IF NOT EXISTS contacts_by_book_and_name
                    ON contacts(address_book_id, display_name COLLATE NOCASE);
                ",
            )
            .context("Could not create database tables")?;

        Ok(Self { connection })
    }

    /// Creates an addressbook after rejecting names that would be unusable in the UI.
    pub fn create_address_book(&self, name: &str) -> Result<i64> {
        if name.trim().is_empty() {
            bail!("An addressbook name cannot be empty.");
        }

        self.connection
            .execute(
                "INSERT INTO address_books (name) VALUES (?1)",
                [name.trim()],
            )
            .with_context(|| format!("Could not create addressbook '{name}'"))?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Returns addressbooks in a stable display order.
    pub fn list_address_books(&self) -> Result<Vec<AddressBook>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM address_books ORDER BY name COLLATE NOCASE, id")
            .context("Could not prepare addressbook query")?;
        let rows = statement
            .query_map([], |row| {
                Ok(AddressBook {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .context("Could not read addressbooks")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load addressbooks")
    }

    /// Counts all addressbooks in this database.
    pub fn count_address_books(&self) -> Result<i64> {
        self.connection
            .query_row("SELECT count(*) FROM address_books", [], |row| row.get(0))
            .context("Could not count addressbooks")
    }

    /// Removes an addressbook only when it has no contacts.
    pub fn delete_address_book(&self, id: i64) -> Result<bool> {
        let deleted = self
            .connection
            .execute(
                "DELETE FROM address_books
                 WHERE id = ?1
                   AND NOT EXISTS (
                       SELECT 1 FROM contacts WHERE address_book_id = ?1
                   )",
                [id],
            )
            .with_context(|| format!("Could not delete addressbook {id}"))?;
        Ok(deleted != 0)
    }

    /// Stores all contact fields as JSON and caches a derived name for list ordering.
    pub fn add_contact(&self, address_book_id: i64, fields: &ContactFields) -> Result<i64> {
        let fields_json =
            serde_json::to_string(fields).context("Could not encode contact fields")?;
        let display_name = fields.display_name();
        self.connection
            .execute(
                "INSERT INTO contacts
                    (address_book_id, display_name, fields_json)
                 VALUES (?1, ?2, ?3)",
                params![address_book_id, display_name, fields_json],
            )
            .context("Could not save contact. Check that the selected addressbook still exists.")?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Imports contacts in one transaction so a failed batch leaves no partial import.
    pub fn add_contacts(&self, address_book_id: i64, contacts: &[ContactFields]) -> Result<usize> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("Could not start contact import")?;
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO contacts
                        (address_book_id, display_name, fields_json)
                     VALUES (?1, ?2, ?3)",
                )
                .context("Could not prepare contact import")?;
            for fields in contacts {
                let fields_json =
                    serde_json::to_string(fields).context("Could not encode imported contact")?;
                statement
                    .execute(params![address_book_id, fields.display_name(), fields_json])
                    .context("Could not save an imported contact")?;
            }
        }
        transaction
            .commit()
            .context("Could not commit imported contacts")?;
        Ok(contacts.len())
    }

    /// Updates a previously loaded contact without replacing its database identity.
    pub fn update_contact(&self, contact: &Contact) -> Result<()> {
        let fields_json =
            serde_json::to_string(&contact.fields).context("Could not encode contact fields")?;
        let display_name = contact.fields.display_name();
        let updated = self
            .connection
            .execute(
                "UPDATE contacts
                 SET display_name = ?1, fields_json = ?2
                 WHERE id = ?3 AND address_book_id = ?4",
                params![
                    display_name,
                    fields_json,
                    contact.id,
                    contact.address_book_id
                ],
            )
            .with_context(|| format!("Could not update contact {}", contact.id))?;

        if updated == 0 {
            bail!("This contact no longer exists. Refresh the contact list and try again.");
        }
        Ok(())
    }

    /// Searches serialized contact fields, optionally restricting results to one addressbook.
    pub fn search_contacts(
        &self,
        address_book_id: Option<i64>,
        query: &str,
    ) -> Result<Vec<Contact>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, address_book_id, display_name, fields_json
                 FROM contacts
                 WHERE (?1 IS NULL OR address_book_id = ?1)
                   AND (
                       ?2 = ''
                       OR instr(lower(display_name), lower(?2)) > 0
                       OR instr(lower(fields_json), lower(?2)) > 0
                   )
                 ORDER BY display_name COLLATE NOCASE, id",
            )
            .context("Could not prepare contact search")?;
        let rows = statement
            .query_map(params![address_book_id, query.trim()], contact_from_row)
            .context("Could not search contacts")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load matching contacts")
    }

    /// Deletes a contact, returning false when it was already removed.
    pub fn delete_contact(&self, id: i64) -> Result<bool> {
        let deleted = self
            .connection
            .execute("DELETE FROM contacts WHERE id = ?1", [id])
            .with_context(|| format!("Could not delete contact {id}"))?;
        Ok(deleted != 0)
    }

    /// Deletes multiple contacts atomically, returning the number that existed.
    pub fn delete_contacts(&self, ids: &[i64]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let transaction = self
            .connection
            .unchecked_transaction()
            .context("Could not start contact deletion")?;
        let deleted_count = {
            let mut statement = transaction
                .prepare("DELETE FROM contacts WHERE id = ?1")
                .context("Could not prepare contact deletion")?;
            ids.iter().try_fold(0, |count, id| {
                statement
                    .execute([id])
                    .map(|deleted| count + deleted)
                    .with_context(|| format!("Could not delete contact {id}"))
            })?
        };
        transaction
            .commit()
            .context("Could not commit contact deletion")?;
        Ok(deleted_count)
    }

    /// Moves multiple contacts to an existing addressbook atomically.
    pub fn move_contacts(&self, ids: &[i64], address_book_id: i64) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let transaction = self
            .connection
            .unchecked_transaction()
            .context("Could not start contact move")?;
        let address_book_exists = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM address_books WHERE id = ?1)",
                [address_book_id],
                |row| row.get::<_, bool>(0),
            )
            .context("Could not verify the destination addressbook")?;
        if !address_book_exists {
            bail!("The destination addressbook does not exist");
        }

        let moved_count = {
            let mut statement = transaction
                .prepare(
                    "UPDATE contacts
                     SET address_book_id = ?1
                     WHERE id = ?2 AND address_book_id != ?1",
                )
                .context("Could not prepare contact move")?;
            ids.iter().try_fold(0, |count, id| {
                statement
                    .execute(params![address_book_id, id])
                    .map(|moved| count + moved)
                    .with_context(|| format!("Could not move contact {id}"))
            })?
        };
        transaction
            .commit()
            .context("Could not commit contact move")?;
        Ok(moved_count)
    }

    /// Counts contacts, optionally within one addressbook.
    pub fn count_contacts(&self, address_book_id: Option<i64>) -> Result<i64> {
        self.connection
            .query_row(
                "SELECT count(*) FROM contacts
                 WHERE (?1 IS NULL OR address_book_id = ?1)",
                [address_book_id],
                |row| row.get(0),
            )
            .context("Could not count contacts")
    }
}

fn cleanup_incomplete_database(path: &Path, error: anyhow::Error) -> anyhow::Error {
    if let Err(cleanup_error) = fs::remove_file(path) {
        if cleanup_error.kind() != std::io::ErrorKind::NotFound {
            return error.context(format!(
                "Could not remove the incomplete database at {}: {cleanup_error}",
                path.display()
            ));
        }
    }
    error
}

impl ContactFields {
    /// Builds a readable name, falling back to the file-as value or first email.
    pub fn display_name(&self) -> String {
        if let Some(display_name) = self
            .name
            .display_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            return display_name.to_owned();
        }

        let name = [
            self.name.prefix.as_deref(),
            self.name.given_name.as_deref(),
            self.name.middle_name.as_deref(),
            self.name.family_name.as_deref(),
            self.name.suffix.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");

        if !name.is_empty() {
            return name;
        }

        self.name
            .file_as
            .as_deref()
            .or(self.name.nickname.as_deref())
            .or_else(|| self.emails.first().map(|email| email.value.as_str()))
            .unwrap_or_default()
            .to_owned()
    }
}

/// Converts stored contact JSON into the typed, provider-neutral model.
fn contact_from_row(row: &Row<'_>) -> rusqlite::Result<Contact> {
    let fields_json: String = row.get(3)?;
    let fields = serde_json::from_str(&fields_json)
        .map_err(|error| Error::FromSqlConversionFailure(3, Type::Text, Box::new(error)))?;

    Ok(Contact {
        id: row.get(0)?,
        address_book_id: row.get(1)?,
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_postal_address_data_defaults_house_number() -> Result<()> {
        let address: PostalAddress =
            serde_json::from_str(r#"{"label":"Home","street":"Main Street"}"#)?;

        assert_eq!(address.street.as_deref(), Some("Main Street"));
        assert_eq!(address.house_number, None);
        Ok(())
    }

    #[test]
    fn creates_and_reopens_database_files() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("addresses.sqlite");

        let database = Database::create_new(&path)?;
        assert_eq!(database.count_address_books()?, 0);
        database.create_address_book("Personal")?;
        drop(database);

        let database = Database::open_existing(&path)?;
        assert_eq!(database.count_address_books()?, 1);
        assert_eq!(database.count_contacts(None)?, 0);
        Ok(())
    }

    #[test]
    fn importing_contacts_is_atomic() -> Result<()> {
        let database = Database::in_memory()?;
        let address_book_id = database.create_address_book("Personal")?;
        database.connection.execute_batch(
            "CREATE TRIGGER reject_imported_contact
             BEFORE INSERT ON contacts
             WHEN NEW.display_name = 'Reject'
             BEGIN
                 SELECT RAISE(ABORT, 'rejected test contact');
             END;",
        )?;
        let contacts = [
            ContactFields {
                name: ContactName {
                    display_name: Some("Accepted".to_owned()),
                    ..ContactName::default()
                },
                ..ContactFields::default()
            },
            ContactFields {
                name: ContactName {
                    display_name: Some("Reject".to_owned()),
                    ..ContactName::default()
                },
                ..ContactFields::default()
            },
        ];

        assert!(database.add_contacts(address_book_id, &contacts).is_err());
        assert_eq!(database.count_contacts(None)?, 0);

        database
            .connection
            .execute_batch("DROP TRIGGER reject_imported_contact;")?;
        assert_eq!(database.add_contacts(address_book_id, &contacts)?, 2);
        assert_eq!(database.count_contacts(Some(address_book_id))?, 2);
        Ok(())
    }

    #[test]
    fn searches_contacts_within_selected_address_book() -> Result<()> {
        let database = Database::in_memory()?;
        let personal_id = database.create_address_book("Personal")?;
        let work_id = database.create_address_book("Work")?;
        let mut personal_contact = ContactFields::default();
        personal_contact.name.display_name = Some("Alex Morgan".into());
        database.add_contact(personal_id, &personal_contact)?;
        let mut work_contact = ContactFields::default();
        work_contact.name.display_name = Some("Morgan Lee".into());
        database.add_contact(work_id, &work_contact)?;

        assert_eq!(database.search_contacts(None, "Morgan")?.len(), 2);
        assert_eq!(
            database.search_contacts(Some(personal_id), "Morgan")?[0]
                .fields
                .display_name(),
            "Alex Morgan"
        );
        assert!(database.search_contacts(Some(work_id), "Alex")?.is_empty());
        Ok(())
    }

    #[test]
    fn creating_a_database_does_not_overwrite_existing_files() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("addresses.sqlite");
        fs::write(&path, b"keep existing data")?;

        assert!(Database::create_new(&path).is_err());
        assert_eq!(fs::read(path)?, b"keep existing data");
        Ok(())
    }

    #[test]
    fn opening_a_missing_database_does_not_create_it() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("missing.sqlite");

        assert!(Database::open_existing(&path).is_err());
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn opening_an_unrelated_sqlite_file_does_not_modify_its_schema() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("other.sqlite");
        let connection = Connection::open(&path)?;
        connection.execute_batch("CREATE TABLE unrelated (id INTEGER PRIMARY KEY);")?;
        drop(connection);

        assert!(Database::open_existing(&path).is_err());

        let connection = Connection::open(&path)?;
        let address_book_table_count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'address_books'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(address_book_table_count, 0);
        Ok(())
    }

    /// Confirms common fields stay searchable and arbitrary imported fields survive a round trip.
    #[test]
    fn saves_updates_and_searches_contacts() -> Result<()> {
        let database = Database::in_memory()?;
        let address_book_id = database.create_address_book("Personal")?;
        let mut fields = ContactFields::default();
        fields.name.display_name = Some("Ada Lovelace".into());
        fields.name.given_name = Some("Ada".into());
        fields.name.family_name = Some("Lovelace".into());
        fields.name.initials = Some("A".into());
        fields.emails.push(LabeledValue {
            label: Some("Work".into()),
            value: "ada@example.com".into(),
        });
        fields.emails.push(LabeledValue {
            label: Some("Home".into()),
            value: "ada.home@example.com".into(),
        });
        fields.phones.push(LabeledValue {
            label: Some("Mobile".into()),
            value: "+1-555-0100".into(),
        });
        fields.addresses.push(PostalAddress {
            label: Some("Work".into()),
            street: Some("1 Analytical Engine Way".into()),
            city: Some("London".into()),
            country: Some("United Kingdom".into()),
            ..PostalAddress::default()
        });
        fields.birthday = Some("1815-12-10".into());
        fields.organization.company = Some("Example Ltd".into());
        fields.extra_fields.insert(
            "Provider-specific value".into(),
            Value::String("preserved".into()),
        );
        let expected_fields = fields.clone();

        let contact_id = database.add_contact(address_book_id, &fields)?;

        let mut contact = database
            .search_contacts(Some(address_book_id), "ADA")?
            .into_iter()
            .next()
            .context("Expected the saved contact to be searchable")?;
        assert_eq!(contact.id, contact_id);
        assert_eq!(contact.fields.emails[0].value, "ada@example.com");
        assert_eq!(
            contact.fields.extra_fields["Provider-specific value"],
            "preserved"
        );
        assert_eq!(contact.fields, expected_fields);
        assert_eq!(contact.fields.display_name(), "Ada Lovelace");
        assert_eq!(
            database
                .search_contacts(None, "ada.home@example.com")?
                .len(),
            1
        );

        contact.fields.name.family_name = Some("Byron".into());
        contact.fields.name.display_name = None;
        database.update_contact(&contact)?;
        assert_eq!(database.search_contacts(None, "Lovelace")?.len(), 0);
        assert_eq!(database.search_contacts(None, "Byron")?.len(), 1);
        assert_eq!(database.count_contacts(Some(address_book_id))?, 1);

        assert!(database.delete_contact(contact_id)?);
        assert!(!database.delete_contact(contact_id)?);
        assert_eq!(database.count_contacts(None)?, 0);
        Ok(())
    }

    /// Confirms non-empty addressbooks cannot be deleted.
    #[test]
    fn refuses_to_delete_nonempty_address_book() -> Result<()> {
        let database = Database::in_memory()?;
        let address_book_id = database.create_address_book("Work")?;
        let mut fields = ContactFields::default();
        fields.name.given_name = Some("Grace".into());
        fields.name.family_name = Some("Hopper".into());
        let contact_id = database.add_contact(address_book_id, &fields)?;

        assert!(!database.delete_address_book(address_book_id)?);
        assert_eq!(database.list_address_books()?.len(), 1);
        assert_eq!(database.count_contacts(Some(address_book_id))?, 1);

        assert!(database.delete_contact(contact_id)?);
        assert!(database.delete_address_book(address_book_id)?);
        assert!(database.list_address_books()?.is_empty());
        Ok(())
    }

    #[test]
    fn deletes_multiple_contacts_atomically() -> Result<()> {
        let database = Database::in_memory()?;
        let address_book_id = database.create_address_book("Personal")?;
        let mut fields = ContactFields::default();
        fields.name.given_name = Some("Ada".into());
        let first_id = database.add_contact(address_book_id, &fields)?;
        fields.name.given_name = Some("Grace".into());
        let second_id = database.add_contact(address_book_id, &fields)?;
        fields.name.given_name = Some("Katherine".into());
        database.add_contact(address_book_id, &fields)?;

        assert_eq!(database.delete_contacts(&[first_id, second_id])?, 2);
        assert_eq!(database.count_contacts(Some(address_book_id))?, 1);
        assert_eq!(database.delete_contacts(&[first_id, second_id])?, 0);
        assert_eq!(database.delete_contacts(&[])?, 0);
        Ok(())
    }

    #[test]
    fn moves_multiple_contacts_between_address_books() -> Result<()> {
        let database = Database::in_memory()?;
        let source_id = database.create_address_book("Personal")?;
        let destination_id = database.create_address_book("Work")?;
        let mut fields = ContactFields::default();
        fields.name.given_name = Some("Ada".into());
        let first_id = database.add_contact(source_id, &fields)?;
        fields.name.given_name = Some("Grace".into());
        let second_id = database.add_contact(source_id, &fields)?;

        assert_eq!(
            database.move_contacts(&[first_id, second_id], destination_id)?,
            2
        );
        assert_eq!(database.count_contacts(Some(source_id))?, 0);
        assert_eq!(database.count_contacts(Some(destination_id))?, 2);
        assert_eq!(database.move_contacts(&[first_id], destination_id)?, 0);
        assert!(database.move_contacts(&[first_id], i64::MAX).is_err());
        Ok(())
    }

    /// Rejects names that would appear blank in the addressbook selector.
    #[test]
    fn rejects_blank_address_book_names() -> Result<()> {
        let database = Database::in_memory()?;
        assert!(database.create_address_book("  ").is_err());
        assert!(database.list_address_books()?.is_empty());
        Ok(())
    }
}
