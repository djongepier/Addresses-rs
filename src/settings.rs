// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

const SETTINGS_VERSION: u32 = 1;
const SETTINGS_DIRECTORY: &str = "Addresses";
const SETTINGS_FILE: &str = "settings.json";

/// App-wide preferences that must remain available independently of an address database.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    version: u32,
    last_database: Option<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            last_database: None,
        }
    }
}

impl AppSettings {
    /// Loads settings from the platform config directory, creating defaults on first launch.
    pub fn load() -> Result<Self> {
        Self::load_from(&settings_file_path()?)
    }

    /// Saves the current settings to the platform config directory.
    pub fn save(&self) -> Result<()> {
        Self::save_to(&settings_file_path()?, self)
    }

    pub fn last_database(&self) -> Option<&Path> {
        self.last_database.as_deref()
    }

    /// Call after successfully opening a database, then persist with [`Self::save`].
    pub fn set_last_database(&mut self, path: Option<PathBuf>) {
        self.last_database = path;
    }

    fn load_from(path: &Path) -> Result<Self> {
        let contents = match fs::read(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let settings = Self::default();
                Self::save_to(path, &settings)?;
                return Ok(settings);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Could not read settings at {}", path.display()));
            }
        };

        let mut settings: Self = serde_json::from_slice(&contents)
            .with_context(|| format!("Settings at {} are not valid JSON", path.display()))?;
        if settings.version > SETTINGS_VERSION {
            bail!(
                "Settings at {} were created by a newer version of Addresses",
                path.display()
            );
        }

        if settings.version != SETTINGS_VERSION {
            settings.version = SETTINGS_VERSION;
            Self::save_to(path, &settings)?;
        }

        Ok(settings)
    }

    fn save_to(path: &Path, settings: &Self) -> Result<()> {
        let parent = path
            .parent()
            .context("Settings file path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create settings directory {}", parent.display()))?;

        let mut temporary_file = NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "Could not create temporary settings file in {}",
                parent.display()
            )
        })?;
        serde_json::to_writer_pretty(&mut temporary_file, settings)
            .with_context(|| format!("Could not serialize settings for {}", path.display()))?;
        temporary_file.write_all(b"\n")?;
        temporary_file
            .as_file()
            .sync_all()
            .with_context(|| format!("Could not flush settings for {}", path.display()))?;
        temporary_file
            .persist(path)
            .with_context(|| format!("Could not save settings to {}", path.display()))?;
        Ok(())
    }
}

fn settings_file_path() -> Result<PathBuf> {
    let directories =
        BaseDirs::new().context("Could not locate the platform's user configuration directory")?;
    Ok(directories
        .config_dir()
        .join(SETTINGS_DIRECTORY)
        .join(SETTINGS_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_load_creates_default_settings_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);

        let settings = AppSettings::load_from(&path).unwrap();

        assert!(settings.last_database().is_none());
        assert!(path.exists());
        let json: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(json["version"], SETTINGS_VERSION);
    }

    #[test]
    fn save_and_load_preserve_last_database_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        let database_path = directory.path().join("contacts.sqlite");

        AppSettings::load_from(&path).unwrap();
        let mut settings = AppSettings::default();
        settings.set_last_database(Some(database_path.clone()));
        AppSettings::save_to(&path, &settings).unwrap();

        let loaded = AppSettings::load_from(&path).unwrap();

        assert_eq!(loaded.last_database(), Some(database_path.as_path()));
    }

    #[test]
    fn invalid_settings_are_reported_without_overwriting_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(SETTINGS_FILE);
        fs::write(&path, b"{").unwrap();

        assert!(AppSettings::load_from(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), b"{");
    }
}
