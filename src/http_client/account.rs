use std::{collections::HashMap, sync::LazyLock};

use cookie::Cookie as Cookie;
use reqwest::{header::COOKIE, RequestBuilder};
use scraper::{Html, Selector};
use serde::Deserialize;
use log::debug;
use regex::Regex;

use crate::credential_storage::UsacoCredentials;

use super::{HttpClient, HttpClientError, Result};

#[derive(Deserialize)]
struct LoginResponse {
    code: u8
}

pub struct UserInfo {
    username: String,
    email: String,
    first_name: String,
    last_name: String,
    division: String
}

static LOGGED_OUT_RE: LazyLock<Regex

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
        debug!("Refresh login for {}", creds.username);
        if let Some(mut creds) = creds {
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
        if body.contains("<script>window.location = \"index.php\"") {
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
            debug!("Request result: {:?}", result);
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
            .ok_or(HttpClientError::UnexpectedResponse("no fname input"))?;

        let lname = doc.select(&lname_selector)
            .into_iter().next()
            .and_then(|e| e.value().attr("value"))
            .ok_or(HttpClientError::UnexpectedResponse("no lname input"))?;
        
        let email = doc.select(&email_selector)
            .into_iter().next()
            .and_then(|e| e.value().attr("value"))
            .ok_or(HttpClientError::UnexpectedResponse("no email input"))?;

        let mut fields = doc.select(&fields_selector);
        let text1 = fields.next()
            .map(|e| e.text());
        let text2 = fields.next()
            .map(|e| e.text());

        Err(HttpClientError::LoggedOut)
    }
}
