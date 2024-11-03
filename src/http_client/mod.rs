mod account;
//mod problem;
//mod solution;

use std::sync::Arc;

use reqwest::{Client, StatusCode};
use thiserror::Error;

use crate::credential_storage::{CredentialStorage, CredentialStorageError};

pub use account::UserInfo;

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

    #[error("Unexpected or malformed response from USACO backend")]
    UnexpectedResponse(&'static str)
}

type Result<T> = std::result::Result<T, HttpClientError>;

pub struct HttpClient {
    cred_storage: Arc<dyn CredentialStorage>,
    client: Client
}

impl HttpClient {
    pub fn init(cred_storage: Arc<dyn CredentialStorage>) -> Self {
        let client = Client::new();
        Self {
            client,
            cred_storage
        }
    }

    /// test connection to usaco.org
    pub async fn ping(&self) -> Result<bool> {
        let res = self.client
            .get("https://usaco.org")
            .send()
            .await?;
        Ok(res.status() == StatusCode::OK)
    }
}
