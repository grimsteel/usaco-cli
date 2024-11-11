mod status_spinner;
mod auth;
mod preferences;
mod problem;
mod solution;

use thiserror::Error;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use log::{LevelFilter, error};
use indicatif_log_bridge::LogWrapper;
use console::style;
use indicatif::MultiProgress;
use std::{io::stdout, process::ExitCode, sync::Arc};
use crate::{credential_storage::{CredentialStorageError, CredentialStorageSecretService}, http_client::{HttpClient, HttpClientError}, preferences::{DataStore, PreferencesError}};
use status_spinner::StatusSpinner;

/// USACO command-line interface
#[derive(Parser, Debug)]
#[command(version, about, long_about = "USACO command-line interface: supports viewing problem info, automatically testing solutions, and uploading solutions to USACO grading servers.", name = "usaco")]
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
        command: auth::Command
    },
    /// View problem info
    Problem {
        #[command(subcommand)]
        command: problem::Command
    },
    /// Manage, test, and submit solutions
    Solution {
        #[command(subcommand)]
        command: solution::Command
    },
    /// Manage CLI preferences
    Preferences {
        #[command(subcommand)]
        command: Option<preferences::Command>
    },
    /// Generate shell completion files
    Completion {
        shell: Shell
    },
    /// Test connection to USACO servers
    Ping
}

#[derive(Error, Debug)]
pub enum CliError {
    #[error("Preferences store error: {0}")]
    PreferencesError(#[from] PreferencesError),
    #[error("API error: {0}")]
    ApiError(#[from] HttpClientError),
    #[error("Credential storage error: {0}")]
    CredentialStorageError(#[from] CredentialStorageError),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Input error: {0}")]
    InputError(#[from] dialoguer::Error)
}

type Result<T = ()> = std::result::Result<T, CliError>;

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

async fn run_internal(multi: MultiProgress, args: Args) -> Result {
    let cred_storage = Arc::new(CredentialStorageSecretService::init().await?);
    let client = HttpClient::init(cred_storage.clone());
    let prefs = DataStore::new().await?;

    match args.command {
        Command::Ping => {
            let status = StatusSpinner::new("Loading...", &multi);
            if let Some(ping) = client.ping().await? {
                status.finish("USACO servers are online", true);
                // print the ping
                println!(
                    "{} {} {}",
                    style("⧗").bold().cyan().bright(),
                    style("Ping:").dim().cyan(),
                    style(format!("{}ms", ping)).bold().cyan().bright()
                );
            } else {
                status.finish("Cannot connect to USACO servers", false);
            }
        },
        Command::Completion { shell } => {
            let mut command = Args::command();
            let name = command.get_name().to_string();
            generate(shell, &mut command, name, &mut stdout());
        },
        Command::Auth { command } => auth::handle(command, client, cred_storage, multi).await?,
        Command::Problem { command } => problem::handle(command, client, &prefs, multi).await?,
        Command::Solution { command } => solution::handle(command, client, &prefs, multi).await?,
        Command::Preferences { command } => preferences::handle(command, &prefs, multi).await?
    }

    Ok(())
}

pub async fn run() -> ExitCode {
    let (multi, args) = setup_logging();
    if let Err(err) = run_internal(multi, args).await {
        error!("Unexpected error: {}", err);
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
