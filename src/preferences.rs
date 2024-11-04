use serde::{Deserialize, Serialize};
use tokio::{fs::{read, write, try_exists}, process::Command};
use thiserror::Error;
use std::{cell::{Ref, RefMut, RefCell}, path::PathBuf};
use clap::ValueEnum;
use log::debug;

const PREF_FILE_NAME: &'static str = "usaco-cli.json";

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
pub struct PreferencesStore {
    preferences: RefCell<Preferences>,
    filename: String
}

impl PreferencesStore {
    /// Load preferences from the preferences file
    /// Searches in the current directory, then in the nearest git dir
    /// If none exists, create one in the nearest git dir, or if none exists, in the current dir
    pub async fn from_file() -> Result<Self> {
        if try_exists(PREF_FILE_NAME).await? {
            debug!("Loading preferences from current directory");
            Ok(Self {
                preferences: RefCell::new(serde_json::from_slice(&read(PREF_FILE_NAME).await?)?),
                filename: PREF_FILE_NAME.into()
            })
        } else {
            // search in git directory
            if let Ok(result) = Command::new("git")
                .arg("rev-parse")
                .arg("--show-toplevel")
                .output()
                .await
            {
                if result.status.code() == Some(0) {
                    if let Ok(git_dir) = String::from_utf8(result.stdout) {
                        let mut filename = PathBuf::from(git_dir.trim());
                        filename.push(PREF_FILE_NAME);
                        return if try_exists(&filename).await? {
                            debug!("Loading preferences from current git directory: {}", filename.display());
                            Ok(Self {
                                preferences: RefCell::new(serde_json::from_slice(&read(&filename).await?)?),
                                filename: filename.into_os_string().into_string().unwrap()
                            })
                        } else {
                            debug!("Creating preferences in current git directory: {}", filename.display());
                            // create in git dir
                            write(&filename, "{}").await?;
                            Ok(Self {
                                preferences: RefCell::new(Preferences::default()),
                                filename: filename.into_os_string().into_string().unwrap()
                            })
                        };
                    }
                }
            }
            debug!("Creating preferences in current directory");
            
            // create in current dir
            write(PREF_FILE_NAME, "{}").await?;
            Ok(Self {
                preferences: RefCell::new(Preferences::default()),
                filename: PREF_FILE_NAME.into()
            })
        }
    }

    pub async fn save(&self) -> Result<()> {
        let lock = self.read()?;
        let serialized = serde_json::to_vec(&*lock)?;
        write(&self.filename, serialized).await?;
        Ok(())
    }

    pub fn read(&self) -> Result<Ref<'_, Preferences>> {
        self.preferences.try_borrow().map_err(|_| PreferencesError::PrefsLocked)
    }

    pub fn write(&self) -> Result<RefMut<'_, Preferences>> {
        self.preferences.try_borrow_mut().map_err(|_| PreferencesError::PrefsLocked)
    }
}
