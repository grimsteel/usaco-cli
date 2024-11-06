use std::collections::HashMap;

use cookie::Cookie as Cookie;
use reqwest::{header::COOKIE, RequestBuilder};
use scraper::{Html, Selector};
use serde::Deserialize;
use log::debug;

use crate::credential_storage::UsacoCredentials;

use super::{HttpClient, HttpClientError, Result, REDIRECT_RE, IntoResult};

#[derive(Deserialize)]
struct LoginResponse {
    code: u8
}

pub struct UserInfo {
    pub username: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub division: String
}

impl HttpClient {
    /// create a new session with a new login
    pub async fn login(&self, username: String, password: String) -> Result<()> {
        debug!("Login with {}", username);
        let form_data = HashMap::from([
            ("uname", &username),
            ("password", &password)
        ]);

        let res = self.client
            .post("https://usaco.org/current/tpcm/login-session.php")
            .form(&form_data)
            .header("X-Requested-With", "XMLHttpRequest")
            .send()
            .await?;

        // parse the session ID cookie
        let session_id = res.cookies()
            .find(|c| c.name() == "PHPSESSID")
            .map(|s| s.value().into());
        
        let body: LoginResponse = res.json().await?;

        match body.code {
            0 => Err(HttpClientError::InvalidUsernamePassword),
            1 => {
                if let Some(session_id) = session_id {
                    let creds = UsacoCredentials {
                        username,
                        password,
                        session_id
                    };
                    
                    self.cred_storage.store_credentials(&creds).await?;

                    Ok(())
                } else {
                    Err(HttpClientError::UnexpectedResponse("no session cookie"))
                }
            }
            _ => Err(HttpClientError::UnexpectedResponse("unknown login result"))
        }
    }

    /// refresh the login on an existing session
    pub async fn refresh_login(&self) -> Result<UsacoCredentials> {
        let creds = self.cred_storage.get_credentials().await?;
        if let Some(mut creds) = creds {
            debug!("Refresh login for {}", creds.username);
            let form_data = HashMap::from([
                ("uname", &creds.username),
                ("password", &creds.password)
            ]);

            let res = self.client
                .post("https://usaco.org/current/tpcm/login-session.php")
                .form(&form_data)
                .header("X-Requested-With", "XMLHttpRequest")
                .send()
                .await?;

            // parse the session ID cookie (not required for this one)
            let session_id = res.cookies()
                .find(|c| c.name() == "PHPSESSID")
                .map(|s| s.value().into());
            
            let body: LoginResponse = res.json().await?;

            match body.code {
                0 => Err(HttpClientError::InvalidUsernamePassword),
                1 => {
                    if let Some(session_id) = session_id {
                        creds.session_id = session_id;
                        
                        self.cred_storage.store_credentials(&creds).await?;
                    }

                    Ok(creds)
                }
                _ => Err(HttpClientError::UnexpectedResponse("unknown login result"))
            }
        } else {
            Err(HttpClientError::LoggedOut)
        }
    }

    /// make a request with the session ID
    /// returns response body
    async fn authed_request(&self, req: RequestBuilder, creds: &UsacoCredentials) -> Result<String> {
        debug!("Making request {:?} with session {}", req, creds.session_id);
        let res = req
            .header(COOKIE, Cookie::new("PHPSESSID", &creds.session_id).to_string())
            .send().await?;

        let body = res.text().await?;
        if REDIRECT_RE.find(&body).is_some() {
            // session expired
            Err(HttpClientError::SessionExpired)
        } else {
            Ok(body)
        }
    }

    /// make a request with the session ID. reauth if needed
    /// returns response body
    async fn authed_request_retry(&self, req: RequestBuilder) -> Result<String> {
        let creds = self.cred_storage.get_credentials().await?;
        if let Some(creds) = creds {
            let result = self.authed_request(req.try_clone().unwrap(), &creds).await;
            match result {
                Err(HttpClientError::SessionExpired) => {
                    let new_creds = self.refresh_login().await?;
                    self.authed_request(req, &new_creds).await
                }
                r => r
            }
        } else {
            Err(HttpClientError::LoggedOut)
        }
    }

    /// get account info
    pub async fn get_user_info(&self) -> Result<UserInfo> {
        let res = self.authed_request_retry(
            self.client.get("https://usaco.org/index.php?page=editaccount")
        ).await?;

        let doc = Html::parse_document(&res);
        let fname_selector = Selector::parse("input[name=fname]").unwrap();
        let lname_selector = Selector::parse("input[name=lname]").unwrap();
        let email_selector = Selector::parse("input[name=email]").unwrap();
        let fields_selector = Selector::parse("div.field2").unwrap();

        let fname = doc.select(&fname_selector)
            .into_iter().next()
            .and_then(|e| e.value().attr("value"))
            .ir()?;

        let lname = doc.select(&lname_selector)
            .into_iter().next()
            .and_then(|e| e.value().attr("value"))
            .ir()?;
        
        let email = doc.select(&email_selector)
            .into_iter().next()
            .and_then(|e| e.value().attr("value"))
            .ir()?;

        let mut fields = doc.select(&fields_selector);
        let username = fields.next()
            .and_then(|e| e.text().nth(1))
            .map(|s| s.trim())
            .ir()?;
        let division = fields.next()
            .and_then(|e| e.text().nth(1))
            .map(|s| s.trim())
            .ir()?;

        Ok(UserInfo {
            first_name: fname.into(),
            last_name: lname.into(),
            username: username.into(),
            email: email.into(),
            division: division.into()
        })
    }
}
