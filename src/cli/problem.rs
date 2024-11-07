use clap::Subcommand;
use std::error::Error;
use console::style;
use dialoguer::{Input, theme::ColorfulTheme};
use indicatif::MultiProgress;
use super::status_spinner::StatusSpinner;
use crate::{http_client::{HttpClient, HttpClientError}, preferences::DataStore};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Display problem metadata and information
    Info {
        /// Problem ID. Will prompt if not given and if current problem is not set
        id: Option<u64>
    }
}

pub async fn handle(command: Command, client: HttpClient, store: &DataStore, multi: MultiProgress) -> Result<(), Box<dyn Error>> {
    match command {
        Command::Info { id } => {
            let id = if let Some(id) = id {
                id
            } else if let Some(id) = store.read()?.current_problem {
                // use current problem
                id
            } else {
                // prompt
                Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Problem ID:")
                    .interact_text()
                    .unwrap()
            };
            
            let status = StatusSpinner::new("Loading problem...", &multi);
            match client.get_problem(id).await {
                Ok(problem) => {
                    // Print problem header
                    status.finish(&format!(
                        "Loaded {}:",
                        style(format!("problem {}", id))
                            .bold()
                            .bright()
                            .cyan()
                    ), true);
                    // problem name
                    println!("{}", style(problem.name).bold().bright());
                    // contest/division/number
                    println!(
                        "{}",
                        style(format!(
                            "{} {}{}",
                            style(problem.contest).yellow(),
                            style(problem.division.to_ansi()),
                            style(format!(": Problem {}", problem.problem_num)).dim().magenta()
                        )).dim()
                    );
                    
                },
                Err(HttpClientError::ProblemNotFound) => {
                    status.finish(&format!("Problem {} not found", id), false);
                },
                Err(e) => Err(e)?
            }
        },
    }

    Ok(())
}
