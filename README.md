<p align="center">
  <img src="assets/addresses-icon.png" alt="Addresses logo" width="128">
</p>

<h1 align="center">Addresses</h1>

<p align="center">
  An offline-first address book for your desktop.
</p>

<p align="center">
  <a href="https://doc.rust-lang.org/edition-guide/rust-2024/index.html"><img src="https://img.shields.io/badge/Rust-2024%20Edition-000000?logo=rust&amp;logoColor=white" alt="Rust 2024 Edition"></a>
  <img src="https://img.shields.io/badge/MSRV-1.89%2B-orange?logo=rust&amp;logoColor=white" alt="Minimum supported Rust version 1.89">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="MIT License"></a>
  <a href="https://www.sqlite.org/"><img src="https://img.shields.io/badge/Storage-SQLite-003B57?logo=sqlite&amp;logoColor=white" alt="SQLite storage"></a>
</p>


## About

Addresses keeps contact information in a local database that remains
accessible independently of the application.

## AI assistance

AI tools assist with development tasks. The models used may differ from task to
task, so they are not individually listed as project authors. The maintainer
reviews and is responsible for accepted changes.

## Implemented

- Create a local SQLite database with a `Personal` address book, or open an
  existing Addresses database.
- Reopen the last successfully opened database at startup. Versioned settings
  are stored in `settings.json` under the platform's user configuration
  directory.
- Show contact and address-book counts and the active database location on the
  Home screen.
- Create named address books and browse contacts across all books or filter to
  one book.
- Manage address books, move selected contacts between books, and delete
  selected contacts. An address book can only be deleted while it is empty.
- Add, list, search, and edit contacts. The editor supports display, given, and
  family names, organization, multiple labeled email addresses and phone
  numbers, and separate Home and Work addresses with street, house number,
  postal code, city, and country fields.
- Import standard English-header Google Contacts and Outlook CSV files with a
  preview and address-book selection. Common fields are mapped into the contact
  model, and unrecognized columns are preserved. UTF-8 and UTF-16 CSV files are
  supported. Imports are not automatically skipped or merged when they match
  existing contacts.
- Export selected contacts as Google Contacts-compatible CSV or structured
  JSON, including their address-book names and supported contact fields.
- Review likely duplicate pairs across all address books. Matching checks
  shared email addresses or phone numbers, exact names, and similar names with
  a shared organization or postal address. Results are suggestions only;
  contacts are not changed automatically.
- Store structured contact data locally. The data model includes additional
  fields such as websites, messaging addresses, events, relationships,
  categories, and extra fields, but these are not yet exposed in the editor.
- Follow the operating system's light or dark appearance.

## Planned

- Detect likely duplicates during import and provide merge choices.
- Print selected contacts to PDF.
- Create, save, and edit address-label layouts and print them to PDF, including
  A4 dimensions, metric layout options, and common label formats. The Print
  labels screen is currently a placeholder.
- Add import/export options and database backup/restore to Settings.

## Stack and dependencies

- Rust and GPUI for the desktop application
- SQLite storage through rusqlite with the SQLite library bundled
- serde and serde_json for contact and settings data
- directories for platform-specific configuration paths
- tempfile for atomic settings-file replacement
- csv for contact import and planned export
- printpdf for planned address-label PDF generation

## Build downloads

GitHub Actions currently builds release executables for Windows x86_64 only
when a version tag such as `v0.5.0` is pushed. Each build creates or updates the
tagged GitHub Release and attaches `addresses-windows-x86_64.zip`; download it
from the release's **Assets** section.

Linux release builds are paused because the Ubuntu 24.04 build failed compiling
`xattr` 0.2.3: it references `libc::ENOATTR`, which is not defined for that
Linux target. See [the failed Linux build](https://github.com/djongepier/Addresses-rs/actions/runs/43850105167)
for the compiler error.

## Screen layout

### Home

Implemented:
- Create a database on first use or open an existing one.
- Show contact and address-book counts and the active database location.
- Open duplicate review from the Home screen.

Planned:
- Show a duplicate count and provide contact search from the dashboard.

### Contacts

Implemented:
- List and search contacts.
- Filter across all address books or within one selected book.
- Create address books and add or edit contacts.
- Manage address books, move selected contacts between books, and delete
  selected contacts. Address books must be empty before they can be deleted.
- Edit display/given/family names, organization, multiple labeled email
  addresses and phone numbers, separate Home and Work street, house number,
  postal code, city, and country fields, and notes.
- Import standard Google Contacts and Outlook CSV files with preview and
  address-book selection.
- Export selected contacts as Google Contacts-compatible CSV or structured
  JSON from the contact bulk actions.
- Review potential duplicate contacts across all address books and edit either
  contact without making automatic changes.

Planned:
- Print selected contacts to PDF.

### Print labels

Planned:
- Produce PDFs for A4 address-label sheets.
- List, create, and edit saved page definitions, including sticker dimensions,
  rows, columns, margins, gaps, and pitch.
- Include common label formats where available.

### Settings

Implemented:
- Show the last opened database and explain automatic startup reopening.
- Create or open a database.
- Follow the operating system's appearance preference.

Planned:
- Add import/export options and database backup/restore.

### Non-visible screens

Implemented:
- New-contact and edit-contact forms for the currently supported core fields.

Planned:
- Duplicate review and merge.
- Setup/edit screens for PDF label layouts.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
