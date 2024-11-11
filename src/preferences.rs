use super::http_client::Problem;
use clap::ValueEnum;
use directories::ProjectDirs;
use indexmap::IndexMap;
use log::debug;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Ref, RefCell, RefMut},
    path::PathBuf,
};
use thiserror::Error;
use tokio::fs::{create_dir_all, read, try_exists, write};

const PREF_FILE_NAME: &'static str = "config.json";
const CACHE_FILE_NAME: &'static str = "problem-cache.json";

#[derive(Error, Debug)]
pub enum PreferencesError {
    #[error("Preferences parse error")]
    SerdeError(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Preferences locked")]
    PrefsLocked,
}

type Result<T> = std::result::Result<T, PreferencesError>;

/// preferred c++ compiler
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CPPCompiler {
    GCC = 0,
    Clang = 1,
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
    Python = 1,
}

impl Default for Language {
    fn default() -> Self {
        Self::CPP
    }
}

impl Language {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::CPP => "cpp",
            Self::Python => "python",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Preferences {
    #[serde(default)]
    pub current_problem: Option<u64>,
    #[serde(default)]
    pub cpp_compiler: CPPCompiler,
    #[serde(default)]
    pub preferred_language: Language,
    #[serde(default)]
    pub solutions_dir: Option<PathBuf>,
}

type ProblemCache = IndexMap<u64, Problem>;

#[derive(Debug)]
pub struct DataStore {
    preferences: RefCell<Preferences>,
    dirs: ProjectDirs,
    problem_cache: RefCell<ProblemCache>,
}

impl DataStore {
    /// Load preferences from the preferences file
    /// Searches in the current directory, then in the nearest git dir
    /// If none exists, create one in the nearest git dir, or if none exists, in the current dir
    pub async fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "grimsteel", "usaco-cli").unwrap();

        // load prefs
        let config_path = dirs.config_dir().join(PREF_FILE_NAME);
        let preferences = if try_exists(&config_path).await? {
            debug!("Loading preferences from {}", config_path.display());
            RefCell::new(serde_json::from_slice(&read(config_path).await?)?)
        } else {
            debug!("Creating preferences at {}", config_path.display());

            // create in user config dir
            create_dir_all(dirs.config_dir()).await?;
            write(&config_path, "{}").await?;
            RefCell::new(Preferences::default())
        };

        // load cache
        let problem_cache_path = dirs.cache_dir().join(CACHE_FILE_NAME);
        let problem_cache = if try_exists(&problem_cache_path).await? {
            RefCell::new(serde_json::from_slice(&read(problem_cache_path).await?)?)
        } else {
            // empty cache
            RefCell::new(ProblemCache::new())
        };

        Ok(Self {
            preferences,
            dirs,
            problem_cache,
        })
    }

    pub async fn save_prefs(&self) -> Result<()> {
        let lock = self.read()?;
        let serialized = serde_json::to_vec(&*lock)?;
        // write to config dir
        create_dir_all(self.dirs.config_dir()).await?;
        write(&self.dirs.config_dir().join(PREF_FILE_NAME), serialized).await?;
        Ok(())
    }

    pub fn read(&self) -> Result<Ref<'_, Preferences>> {
        self.preferences
            .try_borrow()
            .map_err(|_| PreferencesError::PrefsLocked)
    }

    pub fn write(&self) -> Result<RefMut<'_, Preferences>> {
        self.preferences
            .try_borrow_mut()
            .map_err(|_| PreferencesError::PrefsLocked)
    }

    /// uses an existing borrowed problem cache for efficiency
    async fn save_cache(&self, cache: &ProblemCache) -> Result<()> {
        let serialized = serde_json::to_vec(cache)?;
        // write to cache dir
        create_dir_all(self.dirs.cache_dir()).await?;
        write(&self.dirs.cache_dir().join(CACHE_FILE_NAME), serialized).await?;
        Ok(())
    }

    /// insert a problem into the LRU cache
    pub async fn get_cache(&self, id: u64) -> Result<Option<Ref<Problem>>> {
        let mut lock = self
            .problem_cache
            .try_borrow_mut()
            .map_err(|_| PreferencesError::PrefsLocked)?;
        if let Some(idx) = lock.get_index_of(&id) {
            // move to position 0
            lock.move_index(idx, 0);
            // reborrow as immutable
            drop(lock);
            let lock = self.get_full_cache()?;
            self.save_cache(&*lock).await?;
            // return just the item we care about
            let problem = Ref::filter_map(lock, |l| l.get(&id)).ok();
            Ok(problem)
        } else {
            Ok(None)
        }
    }

    /// get the entire cache
    pub fn get_full_cache(&self) -> Result<Ref<ProblemCache>> {
        self.problem_cache
            .try_borrow()
            .map_err(|_| PreferencesError::PrefsLocked)
    }

    /// insert a problem into the LRU cache
    pub async fn insert_cache(&self, problem: Problem) -> Result<()> {
        let mut lock = self
            .problem_cache
            .try_borrow_mut()
            .map_err(|_| PreferencesError::PrefsLocked)?;
        lock.insert_before(0, problem.id, problem);
        // remove old items
        while lock.len() > 10 {
            lock.shift_remove_index(10);
        }
        self.save_cache(&*lock).await?;
        Ok(())
    }

    /// remove items from the cache
    pub async fn remove_cache(&self, items: Vec<u64>) -> Result<usize> {
        let mut lock = self
            .problem_cache
            .try_borrow_mut()
            .map_err(|_| PreferencesError::PrefsLocked)?;
        let count = if items.len() > 0 {
            let mut i = 0;
            for id in &items {
                if lock.shift_remove(id).is_some() {
                    i += 1;
                }
            }
            i
        } else {
            let len = lock.len();
            lock.clear();
            len
        };
        self.save_cache(&*lock).await?;
        Ok(count)
    }
}
