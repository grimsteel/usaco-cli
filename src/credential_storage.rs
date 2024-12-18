use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use directories::ProjectDirs;
use log::debug;
#[cfg(target_os = "linux")]
use secret_service::{Collection, EncryptionType, Item, SecretService};
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};
use thiserror::Error;
use tokio::fs::{create_dir_all, read, remove_file, try_exists, write};

#[derive(Debug, Serialize, Deserialize)]
pub struct UsacoCredentials {
    pub username: String,
    pub password: String,
    pub session_id: String,
}

#[derive(Error, Debug)]
pub enum CredentialStorageError {
    #[cfg(target_os = "linux")]
    #[error("Secret service error: {0}")]
    SecretService(#[from] secret_service::Error),
    #[error("Password is not valid UTF-8")]
    InvalidPassword,
    #[error("Missing username in secret item")]
    MissingUsername,
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, CredentialStorageError>;

#[async_trait(?Send)]
pub trait CredentialStorage {
    async fn store_credentials(&self, creds: &UsacoCredentials) -> Result<()>;
    async fn get_credentials(&self) -> Result<Option<UsacoCredentials>>;
    async fn clear_credentials(&self) -> Result<()>;

    async fn logged_in(&self) -> Result<bool> {
        Ok(self.get_credentials().await?.is_some())
    }

    fn is_secure(&self) -> bool;
}

#[cfg(target_os = "linux")]
async fn get_secret_storage_provider() -> Option<Arc<dyn CredentialStorage>> {
    CredentialStorageSecretService::init()
        .await
        .ok()
        .map(|s| Arc::new(s) as Arc<dyn CredentialStorage>)
}
#[cfg(not(target_os = "linux"))]
async fn get_secret_storage_provider() -> Option<Arc<dyn CredentialStorage>> {
    None
}

/// Automatically select a credential storage provider
pub async fn autoselect_cred_storage(dirs: &ProjectDirs) -> Arc<dyn CredentialStorage> {
    // try secret storage
    if let Some(provider) = get_secret_storage_provider().await {
        return provider;
    }

    // if all else fails, use plaintext
    Arc::new(CredentialStoragePlaintext::init(dirs))
}

/// Plaintext cred storage provider in the config folder
pub struct CredentialStoragePlaintext {
    filename: PathBuf,
}

impl CredentialStoragePlaintext {
    pub fn init(dirs: &ProjectDirs) -> Self {
        let filename = dirs.config_dir().join("secrets.json");
        Self { filename }
    }
}

#[async_trait(?Send)]
impl CredentialStorage for CredentialStoragePlaintext {
    async fn store_credentials(&self, creds: &UsacoCredentials) -> Result<()> {
        create_dir_all(self.filename.parent().unwrap()).await?;
        write(&self.filename, to_vec(creds)?).await?;
        Ok(())
    }
    async fn clear_credentials(&self) -> Result<()> {
        if try_exists(&self.filename).await? {
            remove_file(&self.filename).await?;
        }
        Ok(())
    }
    async fn get_credentials(&self) -> Result<Option<UsacoCredentials>> {
        Ok(if try_exists(&self.filename).await? {
            let contents = read(&self.filename).await?;
            Some(from_slice(&contents)?)
        } else {
            None
        })
    }
    fn is_secure(&self) -> bool {
        false
    }
}

/// Encrypted cred storage provider using the Linux secret-service D-Bus API
#[cfg(target_os = "linux")]
pub struct CredentialStorageSecretService {
    session: SecretService<'static>,
}

#[cfg(target_os = "linux")]
impl CredentialStorageSecretService {
    pub async fn init() -> Result<Self> {
        let session = SecretService::connect(EncryptionType::Plain).await?;
        Ok(Self { session })
    }

    async fn get_collection<'a>(&'a self) -> Result<Collection<'a>> {
        Ok(self.session.get_default_collection().await?)
    }

    async fn get_item<'a>(&self, collection: &'a Collection<'a>) -> Result<Option<Item<'a>>> {
        let attrs = HashMap::from([("service", "usaco.org")]);
        // get first result
        Ok(collection.search_items(attrs).await?.into_iter().next())
    }
}

#[async_trait(?Send)]
#[cfg(target_os = "linux")]
impl CredentialStorage for CredentialStorageSecretService {
    async fn get_credentials(&self) -> Result<Option<UsacoCredentials>> {
        debug!("Loading credentials");
        let coll = self.get_collection().await?;
        let result = self.get_item(&coll).await?;

        // parse this item
        Ok(if let Some(result) = result {
            let mut result_attrs = result.get_attributes().await?;
            let username = result_attrs
                .remove("username")
                .ok_or(CredentialStorageError::MissingUsername)?;
            let secret = String::from_utf8(result.get_secret().await?)
                .map_err(|_| CredentialStorageError::InvalidPassword)?;

            let split_point = secret
                .find(':')
                .ok_or(CredentialStorageError::InvalidPassword)?;

            let session_id = &secret[..split_point];
            let password = &secret[split_point + 1..];

            Some(UsacoCredentials {
                username,
                password: password.into(),
                session_id: session_id.into(),
            })
        } else {
            None
        })
    }

    async fn clear_credentials(&self) -> Result<()> {
        let coll = self.get_collection().await?;
        let result = self.get_item(&coll).await?;

        if let Some(result) = result {
            result.delete().await?;
        }

        Ok(())
    }

    async fn store_credentials(&self, creds: &UsacoCredentials) -> Result<()> {
        debug!("saving credentials");
        let coll = self.get_collection().await?;

        let attrs = HashMap::from([("service", "usaco.org"), ("username", &creds.username)]);

        // add this item to the secret store
        coll.create_item(
            &format!("Credentials for '{}' on 'usaco.org'", &creds.username),
            attrs,
            &[
                creds.session_id.as_bytes(),
                &[0x3a],
                creds.password.as_bytes(),
            ]
            .concat(),
            true,
            "text/plain",
        )
        .await?;

        Ok(())
    }

    fn is_secure(&self) -> bool {
        true
    }
}
