use super::status_spinner::StatusSpinner;
use crate::{
    http_client::{HttpClient, HttpClientError, Problem},
    preferences::DataStore,
};
use clap::Subcommand;
use console::{style, Color};
use dialoguer::{theme::ColorfulTheme, Input};
use indicatif::MultiProgress;
use std::{future::Future, process::Stdio};
use tokio::process::Command as ProcessCommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Display problem metadata and information
    Info {
        /// Problem ID. Will prompt if not given and if current problem is not set
        id: Option<u64>,
    },
    /// Open a problem in your default web browser
    Open {
        /// Problem ID. Will prompt if not given and if current problem is not set
        id: Option<u64>,
        /// Only display problem URL instead of launching browser
        #[arg(short, long)]
        no_launch_browser: bool,
    },
    /// Manage the LRU problem info cache
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum CacheCommand {
    /// List currently cached problems
    List,
    /// Remove problems from the cache
    Clear {
        /// Problem IDs to remove. Will remove all if not given. Can specify multiple times.
        #[arg(short, long, num_args = 0..)]
        problem_ids: Vec<u64>,
    },
}

fn print_problem(problem: &Problem) {
    // problem name
    println!("\n{}", style(&problem.name).bold().bright().underlined());
    // contest/division/number
    println!(
        "{}\n",
        style(format!(
            "{} {}{}",
            style(&problem.contest).yellow(),
            problem.division.to_ansi(),
            style(format!(": Problem {}", problem.problem_num))
                .dim()
                .magenta()
        ))
        .dim()
    );
    println!("{}", problem.description);
}

pub async fn get_problem<
    'a,
    T: FnMut(Problem) -> R,
    R: Future<Output = super::Result> + Send + Sync + 'a,
>(
    id_param: Option<u64>,
    client: &HttpClient,
    store: &'a DataStore,
    multi: &MultiProgress,
    mut cb: T,
) -> super::Result {
    let id = if let Some(id) = id_param {
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

    // check cache first
    if let Some(cached_problem) = store.get_cache(id).await? {
        // Print problem header
        status.finish(
            &format!(
                "Loaded {}",
                style(format!("problem {}", cached_problem.id))
                    .bold()
                    .bright()
                    .cyan()
            ),
            true,
        );

        cb(cached_problem.clone()).await?;
    } else {
        match client.get_problem(id).await {
            Ok(problem) => {
                // Print problem header
                status.finish(
                    &format!(
                        "Loaded {}",
                        style(format!("problem {}", problem.id))
                            .bold()
                            .bright()
                            .cyan()
                    ),
                    true,
                );

                // insert into cache
                store.insert_cache(problem.clone()).await?;

                cb(problem).await?;
            }
            Err(HttpClientError::ProblemNotFound) => {
                status.finish(&format!("Problem {} not found", id), false);
            }
            Err(e) => Err(e)?,
        }
    }

    Ok(())
}

pub async fn handle(
    command: Command,
    client: HttpClient,
    store: &DataStore,
    multi: MultiProgress,
) -> super::Result {
    match command {
        Command::Info { id } => {
            get_problem(id, &client, store, &multi, |problem| async move {
                print_problem(&problem);
                Ok(())
            })
            .await?;
        }
        Command::Open {
            id,
            no_launch_browser,
        } => {
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
                println!(
                    "{}",
                    style(format!("Opening {}...", style(&problem_url).bold().cyan())).blue()
                );

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
        Command::Cache {
            command: CacheCommand::List,
        } => {
            let items = store.get_full_cache()?;
            // header
            println!("{}", style("Cached problems:").bold().cyan());
            for (i, value) in items.values().enumerate() {
                println!(
                    "{} {} {}",
                    style(format!("{}:", i + 1))
                        .bold()
                        // color the index based on the recency
                        .fg(match i {
                            0..3 => Color::Green,
                            3..6 => Color::Yellow,
                            _ => Color::Red,
                        }),
                    value.name,
                    style(format!("({})", value.id)).magenta()
                );
            }
        }
        Command::Cache {
            command: CacheCommand::Clear { problem_ids },
        } => {
            let count = store.remove_cache(problem_ids).await?;
            println!(
                "{}",
                style(format!("Successfully removed {} items.", count))
                    .green()
                    .bold()
            );
        }
    }

    Ok(())
}
