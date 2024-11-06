use clap::Subcommand;
use std::{sync::Arc, error::Error};
use console::style;
use dialoguer::{Input, theme::ColorfulTheme, Password};
use indicatif::MultiProgress;
use super::status_spinner::StatusSpinner;
use crate::{credential_storage::CredentialStorage, http_client::{HttpClient, HttpClientError, UserInfo}};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Display problem metadata and information
    Info {
        /// Problem ID
        id: u64
    }
}

pub async fn handle(command: Command, client: HttpClient, multi: MultiProgress) -> Result<(), Box<dyn Error>> {
    match command {
        Command::Info { id } => {
            let status = StatusSpinner::new("Loading problem...", &multi);
            match client.get_problem(id).await {
                Ok(problem) => {
                    // Print problem header
                    status.finish(&format!("Loaded {}:", style(format!("problem {}", id)).bold().bright()), true);
                    println!("{:?}", problem);
                },
                Err(HttpClientError::ProblemNotFound) => {
                    status.finish("Problem not found", false);
                },
                Err(e) => Err(e)?
            }
        },
    }

    Ok(())
}
