mod account;
mod problem;
//mod solution;

use std::{
    sync::{Arc, LazyLock},
    time::Instant,
};

use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::credential_storage::{CredentialStorage, CredentialStorageError};

pub use account::UserInfo;
pub use problem::{Problem, IoMode};

#[derive(Error, Debug)]
pub enum HttpClientError {
    #[error("HTTP error")]
    Http(#[from] reqwest::Error),
    #[error("Credential storage error")]
    CredentialStorage(#[from] CredentialStorageError),

    #[error("You are not currently logged in")]
    LoggedOut,

    #[error("Session expired")]
    SessionExpired,
    #[error("Invalid username or password!")]
    InvalidUsernamePassword,

    #[error("Problem not found")]
    ProblemNotFound,

    #[error("Unexpected or malformed response from USACO backend: {0}")]
    UnexpectedResponse(&'static str),
}

type Result<T> = std::result::Result<T, HttpClientError>;

trait IntoResult<T> {
    fn ir(self) -> Result<T>
    where
        Self: Sized,
    {
        self.ir_msg("missing field")
    }
    fn ir_msg(self, msg: &'static str) -> Result<T>;
}

impl<T> IntoResult<T> for Option<T> {
    fn ir_msg(self, msg: &'static str) -> Result<T> {
        match self {
            Self::Some(v) => Ok(v),
            None => Err(HttpClientError::UnexpectedResponse(msg)),
        }
    }
}

static REDIRECT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)<script>\s+window.location ?= ?['"]index.php['"];?\s+</script>"#).unwrap()
});

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize)]
pub enum Division {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

impl Division {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bronze" => Some(Self::Bronze),
            "silver" => Some(Self::Silver),
            "gold" => Some(Self::Gold),
            "platinum" => Some(Self::Platinum),
            _ => None,
        }
    }
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Bronze => "bronze",
            Self::Silver => "silver",
            Self::Gold => "gold",
            Self::Platinum => "platinum",
        }
    }
    /// Color the division with the division colors
    pub fn to_ansi(&self) -> String {
        let div_format = match self {
            Self::Gold => ("Gold", "246;221;138"),
            Self::Silver => ("Silver", "199;199;199"),
            Self::Bronze => ("Bronze", "232;175;140"),
            Self::Platinum => ("Platinum", "207;211;180"),
        };

        format!("\x1b[38;2;{}m{}\x1b[0m", div_format.1, div_format.0)
    }
    /// Return all division names in lowercase
    pub fn get_all() -> [&'static str; 4] {
        ["bronze", "silver", "gold", "platinum"]
    }
}

pub struct HttpClient {
    cred_storage: Arc<dyn CredentialStorage>,
    client: Client,
}

impl HttpClient {
    pub fn init(cred_storage: Arc<dyn CredentialStorage>) -> Self {
        let client = Client::new();
        Self {
            client,
            cred_storage,
        }
    }

    /// test and time connection to usaco.org
    pub async fn ping(&self) -> Result<Option<u128>> {
        let start = Instant::now();
        let res = self.client.get("https://usaco.org").send().await?;
        let time = start.elapsed().as_millis();
        Ok(if res.status() == StatusCode::OK {
            Some(time)
        } else {
            None
        })
    }
}
