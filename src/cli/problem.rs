use super::{status_spinner::StatusSpinner, CliError};
use crate::{
    http_client::{HttpClient, HttpClientError, Problem},
    preferences::DataStore,
};
use clap::Subcommand;
use console::{style, Color};
use dialoguer::{theme::ColorfulTheme, Input};
use indicatif::MultiProgress;
use std::{future::Future, io::{stdin, Read}, process::Stdio};
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
    /// Manually import a problem into the cache
    /// For environments where fetching the problem automatically does not work
    /// Problem content is read from standard input
    Import {
        /// The problem ID of the problem to import
        /// Required. Will not prompt or use current-problem
        id: u64
    }
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

/// Import a problem into the store by parsing problem HTML from stdin
async fn import_problem_stdin(problem_id: u64, client: &HttpClient, store: &DataStore) -> super::Result<String> {
    // read problem HTML from stdin
    let stdin_handle = stdin();
    let mut stdin_lock = stdin_handle.lock();
    let mut input = Vec::new();
    stdin_lock.read_to_end(&mut input)?;
    
    // parse string
    let input = String::from_utf8_lossy(&input).into_owned();

    let problem = client.parse_problem_html(problem_id, input, false).await?;

    let message = format!(
        "Successfully imported {} ({})",
        style(format!("problem {}", problem_id))
            .bold()
            .bright()
            .cyan(),
        problem.name
    );

    // store
    store.insert_cache(problem).await?;

    Ok(message)
} 

pub async fn get_problem<'a, T: FnOnce(Problem) -> R, R: Future<Output = super::Result> + 'a>(
    id_param: Option<u64>,
    client: &HttpClient,
    store: &'a DataStore,
    multi: &MultiProgress,
    cb: T,
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
                return Err(CliError::ExitError);
            }
            Err(e) => Err(e)?,
        }
    }

    Ok(())
}

pub fn open_url(url: &str) -> super::Result {
    // print a styled url
    println!(
        "{}",
        style(format!("Opening {}...", style(&url).bold().cyan())).blue()
    );

    // launch
    if cfg!(target_os = "linux") {
        ProcessCommand::new("xdg-open")
            .arg(url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
    } else if cfg!(target_os = "macos") {
        ProcessCommand::new("open")
            .arg(url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
    } else if cfg!(target_os = "windows") {
        ProcessCommand::new("cmd.exe")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
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
                open_url(&problem_url)?;
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
            command: CacheCommand::Import { id: problem_id },
        } => {
            let status = StatusSpinner::new("Loading problem...", &multi);

            match import_problem_stdin(problem_id, &client, store).await {
                Ok(message) => {
                    status.finish(&message, true);
                }
                Err(err) => {
                    status.finish(&format!("Error importing problem: {}", err), false);
                }
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
