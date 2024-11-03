use clap::Subcommand;
use std::{sync::Arc, error::Error};
use console::style;
use dialoguer::{Input, theme::ColorfulTheme, Password};
use indicatif::MultiProgress;
use super::status_spinner::StatusSpinner;
use crate::{credential_storage::CredentialStorage, http_client::{HttpClient, HttpClientError, UserInfo}};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Log in to your USACO account
    Login {
        /// Username of the account to log in to. Will prompt if not given
        #[arg(short, long)]
        username: Option<String>,
    },
    /// Log out of your USACO account
    Logout,
    /// View authentication status and user information
    Whoami
}

pub async fn handle(command: Command, client: HttpClient, cred_storage: Arc<dyn CredentialStorage>, multi: MultiProgress) -> Result<(), Box<dyn Error>> {
    match command {
        Command::Logout => {
            let status = StatusSpinner::new("Logging out...", &multi);
            cred_storage.clear_credentials().await?;
            status.finish("Logged out", true);
        },
        Command::Login { username } => {
            // make sure they're not already logged in
            if cred_storage.logged_in().await? {
                StatusSpinner::new("", &multi)
                    .finish("You are already logged in!", false);
            } else {
                let user_id = if let Some(username) = username {
                    username
                } else {
                    // prompt
                    Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Username")
                        .interact_text()
                        .unwrap()
                };
                // prompt for password
                let password = Password::with_theme(&ColorfulTheme::default())
                    .with_prompt("Password")
                    .interact()
                    .unwrap();

                let status = StatusSpinner::new(
                    "Logging in...",
                    &multi
                );

                // log in
                match client.login(user_id, password).await {
                    Ok(()) => {
                        status.finish(
                            "Successfully logged in.",
                            true
                        );
                    },
                    Err(HttpClientError::InvalidUsernamePassword) => {
                        status.finish(
                            "Invalid username or password.",
                            false
                        );
                    },
                    e => {
                        e?;
                    }
                }
            }
        },
        Command::Whoami => {
            let status = StatusSpinner::new(
                "Loading account information...",
                &multi
            );

            match client.get_user_info().await {
                Ok(UserInfo { first_name, last_name, email, username, division }) => {
                    status.finish(
                        &format!(
                            "Logged in as {}{}",
                            style("@").bright().cyan(),
                            style(username).bright().cyan().bold()
                        ),
                        true
                    );

                    // print a formatted display
                    
                    println!(
                        "{} {} {}",
                        style("Name:").dim(),
                        style(first_name).bright().magenta(),
                        style(last_name).bright().magenta()
                    );
                    println!(
                        "{} {}",
                        style("Email:").dim(),
                        style(email).bright().blue(),
                    );
                    // Color the division with the division colors
                    let div_format = match division.as_str() {
                        "Gold" => "246;221;138",
                        "Silver" => "199;199;199",
                        "Bronze" => "232;175;140",
                        "Platinum" => "207;211;180",
                        _ => "255;255;255"
                    };
                    println!(
                        "{} \x1b[38;2;{}m{}\x1b[0m",
                        style("Division:").dim(),
                        div_format,
                        style(division).bright()
                    );
                },
                Err(HttpClientError::LoggedOut) => {
                    status.finish(
                        "You are not currently logged in.",
                        false
                    );
                },
                e => {
                    e?;
                }
            }
            
        }
    }

    Ok(())
}
