use std::collections::HashMap;

use async_trait::async_trait;
#[cfg(unix)]
use secret_service::{Collection, EncryptionType, Item, SecretService};
use thiserror::Error;

pub struct UsacoCredentials {
    pub username: String,
    pub password: String,
    pub session_id: String
}

#[derive(Error, Debug)]
pub enum CredentialStorageError {
    #[cfg(unix)]
    #[error("secret service error")]
    SecretService(#[from] secret_service::Error),
    #[error("password is not valid UTF-8")]
    InvalidPassword,
    #[error("missing username in secret item")]
    MissingUsername
}

type Result<T> = std::result::Result<T, CredentialStorageError>;

#[async_trait]
pub trait CredentialStorage {
    async fn store_credentials(&self, creds: &UsacoCredentials) -> Result<()>;
    async fn get_credentials(&self) -> Result<Option<UsacoCredentials>>;
    async fn clear_credentials(&self) -> Result<()>;
}

#[cfg(unix)]
pub struct CredentialStorageSecretService {
    session: SecretService<'static>,
}

#[cfg(unix)]
impl CredentialStorageSecretService {
    pub async fn init() -> Result<Self> {
        let session = SecretService::connect(EncryptionType::Plain).await?;
        Ok(Self {
            session
        })
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

#[async_trait]
#[cfg(unix)]
impl CredentialStorage for CredentialStorageSecretService {
    async fn get_credentials(&self) -> Result<Option<UsacoCredentials>> {
        let coll = self.get_collection().await?;
        let result = self.get_item(&coll).await?;

        // parse this item
        Ok(if let Some(result) = result {
            let mut result_attrs = result.get_attributes().await?;
            let username = result_attrs.remove("username")
                .ok_or(CredentialStorageError::MissingUsername)?;
            let secret = String::from_utf8(result.get_secret().await?)
                .map_err(|_| CredentialStorageError::InvalidPassword)?;

            let (password, session_id) = secret.split_at(secret.find(':').ok_or(CredentialStorageError::InvalidPassword)?);

            Some(UsacoCredentials { username, password: password.into(), session_id: session_id.into() })
        } else {
            None
        })
    }

    async fn clear_credentials(&self) -> Result<()> {
        let coll = self.get_collection().await?;
        let result = self.get_item(&coll).await?;

        if let Some(result) = result { result.delete().await?; }

        Ok(())
    }

    async fn store_credentials(&self, creds: &UsacoCredentials) -> Result<()> {
        let coll = self.get_collection().await?;
        
        let attrs = HashMap::from([
            ("service", "usaco.org"),
            ("username", &creds.username)
        ]);

        // add this item to the secret store
        coll.create_item(
            &format!("Credentials for '{}' on 'usaco.org'", &creds.username),
            attrs,
            &[creds.session_id.as_bytes(), &[0x3a], creds.password.as_bytes()].concat(),
            true,
            "text/plain"
        ).await?;

        Ok(())
    }
}

