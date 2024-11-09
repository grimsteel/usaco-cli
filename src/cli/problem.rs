use clap::Subcommand;
use std::{error::Error, process::Stdio};
use console::style;
use dialoguer::{Input, theme::ColorfulTheme};
use indicatif::MultiProgress;
use tokio::process::Command as ProcessCommand;
use super::status_spinner::StatusSpinner;
use crate::{http_client::{HttpClient, HttpClientError}, preferences::DataStore};

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Display problem metadata and information
    Info {
        /// Problem ID. Will prompt if not given and if current problem is not set
        id: Option<u64>
    },
    /// Open a problem in your default web browser
    Open {
        /// Problem ID. Will prompt if not given and if current problem is not set
        id: Option<u64>,
        /// Only display problem URL instead of launching browser
        #[arg(short, long)]
        no_launch_browser: bool
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
                    println!("\n{}", style(problem.name).bold().bright().underlined());
                    // contest/division/number
                    println!(
                        "{}\n",
                        style(format!(
                            "{} {}{}",
                            style(problem.contest).yellow(),
                            problem.division.to_ansi(),
                            style(format!(": Problem {}", problem.problem_num)).dim().magenta()
                        )).dim()
                    );
                    println!("{}", problem.description);
                },
                Err(HttpClientError::ProblemNotFound) => {
                    status.finish(&format!("Problem {} not found", id), false);
                },
                Err(e) => Err(e)?
            }
        },
        Command::Open { id, no_launch_browser } => {
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

            let problem_url = format!("https://usaco.org/index.php?page=viewproblem2&cpid={}", id);

            if no_launch_browser {
                // print a plain url
                println!("{}", problem_url);
            } else {
                // print a styled url
                println!("{}", style(format!("Opening {}...", style(&problem_url).bold().cyan())).blue());

                // launch
                // TODO: mac/windows support
                ProcessCommand::new("xdg-open")
                    .arg(problem_url)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?;
            }
        }
    }

    Ok(())
}
