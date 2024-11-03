use serde::{Deserialize, Serialize};
use tokio::{fs::{read, write, try_exists}, process::Command};
use thiserror::Error;
use std::{path::PathBuf, ffi::OsString, os::unix::ffi::OsStringExt};
use log::debug;

const PREF_FILE_NAME: &'static str = "usaco-cli.json";

#[derive(Error, Debug)]
pub enum PreferencesError {
    #[error("Preferences parse error")]
    SerdeError(#[from] serde_json::Error),
    #[error("I/O error")]
    IOError(#[from] std::io::Error)
}

type Result<T> = std::result::Result<T, PreferencesError>;

/// preferred c++ compiler
#[derive(Serialize, Deserialize, Debug)]
pub enum CPPCompiler {
    GCC,
    Clang
}

impl Default for CPPCompiler {
    fn default() -> Self {
        Self::GCC
    }
}

/// preferred language for boilerplate code
#[derive(Serialize, Deserialize, Debug)]
pub enum Language {
    CPP,
    Python
}

impl Default for Language {
    fn default() -> Self {
        Self::CPP
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Preferences {
    pub current_problem: Option<u64>,
    pub cpp_compiler: CPPCompiler,
    pub preferred_language: Language
}


impl Preferences {
    /// Load preferences from the preferences file
    /// Searches in the current directory, then in the nearest git dir
    /// If none exists, create one in the nearest git dir, or if none exists, in the current dir
    pub async fn from_file() -> Result<Self> {
        if try_exists(PREF_FILE_NAME).await? {
            debug!("Loading preferences from current directory");
            Ok(serde_json::from_slice(&read(PREF_FILE_NAME).await?)?)
        } else {
            // search in git directory
            if let Ok(result) = Command::new("git")
                .arg("rev-parse")
                .arg("--show-toplevel")
                .output()
                .await
            {
                if result.status.code() == Some(0) {
                    let mut filename = PathBuf::from(OsString::from_vec(result.stdout));
                    filename.push(PREF_FILE_NAME);
                    return if try_exists(&filename).await? {
                        debug!("Loading preferences from current git directory");
                        Ok(serde_json::from_slice(&read(&filename).await?)?)
                    } else {
                        debug!("Creating preferences in current git directory");
                        // create in git dir
                        write(&filename, "{}").await?;
                        Ok(Self::default())
                    };
                }
            }
            debug!("Creating preferences in current directory");
            
            // create in current dir
            write(PREF_FILE_NAME, "{}").await?;
            Ok(Self::default())
        }
    }
}
