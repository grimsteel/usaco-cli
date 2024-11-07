use serde::{Deserialize, Serialize};
use tokio::fs::{read, write, try_exists, create_dir_all};
use thiserror::Error;
use std::{cell::{Ref, RefMut, RefCell}, path::PathBuf};
use clap::ValueEnum;
use directories::ProjectDirs;
use log::debug;

const PREF_FILE_NAME: &'static str = "config.json";

#[derive(Error, Debug)]
pub enum PreferencesError {
    #[error("Preferences parse error")]
    SerdeError(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Preferences locked")]
    PrefsLocked
}

type Result<T> = std::result::Result<T, PreferencesError>;

/// preferred c++ compiler
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CPPCompiler {
    GCC = 0,
    Clang = 1
}

impl Default for CPPCompiler {
    fn default() -> Self {
        Self::GCC
    }
}

/// preferred language for boilerplate code
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Language {
    CPP = 0,
    Python = 1
}

impl Default for Language {
    fn default() -> Self {
        Self::CPP
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Preferences {
    #[serde(default)]
    pub current_problem: Option<u64>,
    #[serde(default)]
    pub cpp_compiler: CPPCompiler,
    #[serde(default)]
    pub preferred_language: Language
}

#[derive(Debug)]
pub struct DataStore {
    preferences: RefCell<Preferences>,
    dirs: ProjectDirs
}

impl DataStore {
    /// Load preferences from the preferences file
    /// Searches in the current directory, then in the nearest git dir
    /// If none exists, create one in the nearest git dir, or if none exists, in the current dir
    pub async fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "grimsteel", "usaco-cli").unwrap();
        let config_path = dirs.config_dir().join(PREF_FILE_NAME);
        if try_exists(&config_path).await? {
            debug!("Loading preferences from {}", config_path.display());
            Ok(Self {
                preferences: RefCell::new(serde_json::from_slice(&read(config_path).await?)?),
                dirs
            })
        } else {
            debug!("Creating preferences at {}", config_path.display());
            
            // create in user config dir
            create_dir_all(dirs.config_dir()).await?;
            write(&config_path, "{}").await?;
            Ok(Self {
                preferences: RefCell::new(Preferences::default()),
                dirs
            })
        }
    }

    pub async fn save(&self) -> Result<()> {
        let lock = self.read()?;
        let serialized = serde_json::to_vec(&*lock)?;
        // write to config dir
        create_dir_all(self.dirs.config_dir()).await?;
        write(&self.dirs.config_dir().join(PREF_FILE_NAME), serialized).await?;
        Ok(())
    }

    pub fn read(&self) -> Result<Ref<'_, Preferences>> {
        self.preferences.try_borrow().map_err(|_| PreferencesError::PrefsLocked)
    }

    pub fn write(&self) -> Result<RefMut<'_, Preferences>> {
        self.preferences.try_borrow_mut().map_err(|_| PreferencesError::PrefsLocked)
    }
}
