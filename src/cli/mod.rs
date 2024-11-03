mod status_spinner;

use clap::{Parser, Subcommand};
use log::{LevelFilter, error};
use indicatif_log_bridge::LogWrapper;
use console::style;
use indicatif::MultiProgress;
use dialoguer::{Input, Password, theme::ColorfulTheme};
use std::{sync::Arc, error::Error};
use crate::{http_client::{HttpClientError, HttpClient, UserInfo}, credential_storage::{CredentialStorage, CredentialStorageSecretService}};
use status_spinner::StatusSpinner;

/// USACO command-line interface
#[derive(Parser, Debug)]
#[command(version, about, long_about = "USACO command-line interface: supports viewing problem info, automatically testing solutions, and uploading solutions to USACO grading servers.")]
struct Args {
    /// Maximum logging level
    #[arg(short, long, value_enum)]
    log_level: Option<LevelFilter>,
    
    #[command(subcommand)]
    command: Command
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage USACO account authentication
    Auth {
        #[command(subcommand)]
        command: AuthCommand
    },
    /// Test connection to USACO servers
    Ping
}
#[derive(Subcommand, Debug)]
enum AuthCommand {
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

fn setup_logging() -> (MultiProgress, Args) {
    let mut logger = env_logger::Builder::from_default_env();
    let args = Args::parse();

    if let Some(level) = args.log_level {
        logger.filter_level(level);
    }

    let multi = MultiProgress::new();
    let logger = logger.build();
    let log_filter = logger.filter();
    LogWrapper::new(multi.clone(), logger)
        .try_init()
        .unwrap();
    log::set_max_level(log_filter);

    (multi, args)
}

async fn run_internal(multi: MultiProgress, args: Args) -> Result<(), Box<dyn Error>> {
    let cred_storage = Arc::new(CredentialStorageSecretService::init().await?);
    let client = HttpClient::init(cred_storage.clone());

    match args.command {
        Command::Ping => {
            let status = StatusSpinner::new("Loading...", &multi);
            if client.ping().await? {
                status.finish("USACO servers are online", true);
            } else {
                status.finish("Cannot connect to USACO servers", false);
            }
        },
        Command::Auth { command } => {
            match command {
                AuthCommand::Logout => {
                    let status = StatusSpinner::new("Logging out...", &multi);
                    cred_storage.clear_credentials().await?;
                    status.finish("Logged out", true);
                },
                AuthCommand::Login { username } => {
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
                AuthCommand::Whoami => {
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
        }
    }

    Ok(())
}

pub async fn run() {
    let (multi, args) = setup_logging();
    if let Err(err) = run_internal(multi, args).await {
        error!("Unexpected error: {}", err);
    }
}
