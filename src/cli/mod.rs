mod auth;
mod preferences;
mod problem;
mod solution;
mod status_spinner;

use crate::{
    credential_storage::{autoselect_cred_storage, CredentialStorageError},
    http_client::{HttpClient, HttpClientError},
    preferences::{DataStore, PreferencesError},
};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use console::style;
use directories::ProjectDirs;
use env_logger::Env;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use log::{error, Level, LevelFilter};
use status_spinner::StatusSpinner;
use std::{
    io::{stdout, Write},
    process::ExitCode,
};
use thiserror::Error;

/// USACO command-line interface
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "USACO command-line interface: supports viewing problem info, automatically testing solutions, and uploading solutions to USACO grading servers.",
    name = "usaco"
)]
struct Args {
    /// Maximum logging level
    #[arg(short, long, value_enum)]
    log_level: Option<LevelFilter>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage USACO account authentication
    Auth {
        #[command(subcommand)]
        command: auth::Command,
    },
    /// View problem info
    Problem {
        #[command(subcommand)]
        command: problem::Command,
    },
    /// Manage, test, and submit solutions
    Solution {
        #[command(subcommand)]
        command: solution::Command,
    },
    /// Manage CLI preferences
    Preferences {
        #[command(subcommand)]
        command: Option<preferences::Command>,
    },
    /// Generate shell completion files
    Completion { shell: Shell },
    /// Test connection to USACO servers
    Ping,
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
    InputError(#[from] dialoguer::Error),

    /// used when the message has already been printed and we just need to exit
    #[error("")]
    ExitError,
}

type Result<T = ()> = std::result::Result<T, CliError>;

fn setup_logging() -> (MultiProgress, Args) {
    let mut logger = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    let show_line_numbers =
        std::env::var("RUST_LOG_LINE_NUMBERS").is_ok_and(|s| s.to_lowercase() == "true");
    // set style
    logger.format(move |buf, record| {
        let level_icon = match record.level() {
            Level::Error => "✕",
            Level::Warn => "⚠",
            Level::Info => "i",
            Level::Debug => "DBG",
            Level::Trace => "TRACE",
        };
        if show_line_numbers {
            // print filename and line numbers
            writeln!(
                buf,
                "{0}{1}{0:#} ({3}:{4}): {2}",
                buf.default_level_style(record.level()).bold(),
                level_icon,
                record.args(),
                record.file().unwrap_or("?"),
                record
                    .line()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "?".to_string())
            )
        } else {
            writeln!(
                buf,
                "{0}{1}:{0:#} {2}",
                buf.default_level_style(record.level()).bold(),
                level_icon,
                record.args()
            )
        }
    });

    let args = Args::parse();

    if let Some(level) = args.log_level {
        logger.filter_level(level);
    }

    let multi = MultiProgress::new();
    let logger = logger.build();
    let log_filter = logger.filter();
    LogWrapper::new(multi.clone(), logger).try_init().unwrap();
    log::set_max_level(log_filter);

    (multi, args)
}

async fn run_internal(multi: MultiProgress, args: Args) -> Result {
    let dirs = ProjectDirs::from("com", "grimsteel", "usaco-cli").unwrap();
    let prefs = DataStore::new(dirs.clone()).await?;
    let cred_storage = autoselect_cred_storage(&dirs).await;
    let client = HttpClient::init(cred_storage.clone());

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
        }
        Command::Completion { shell } => {
            let mut command = Args::command();
            let name = command.get_name().to_string();
            generate(shell, &mut command, name, &mut stdout());
        }
        Command::Auth { command } => auth::handle(command, client, cred_storage, multi).await?,
        Command::Problem { command } => problem::handle(command, client, &prefs, multi).await?,
        Command::Solution { command } => {
            solution::handle(command, client, &prefs, multi, dirs).await?
        }
        Command::Preferences { command } => preferences::handle(command, &prefs, multi).await?,
    }

    Ok(())
}

pub async fn run() -> ExitCode {
    let (multi, args) = setup_logging();
    if let Err(err) = run_internal(multi, args).await {
        if !matches!(err, CliError::ExitError) {
            error!("Unexpected error: {}", err);
        }
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
