mod status_spinner;
mod auth;
mod preferences;

use clap::{Parser, Subcommand};
use log::{LevelFilter, error};
use indicatif_log_bridge::LogWrapper;
use console::style;
use indicatif::MultiProgress;
use std::{sync::Arc, error::Error};
use crate::{http_client::HttpClient, credential_storage::CredentialStorageSecretService, preferences::PreferencesStore};
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
        command: auth::Command
    },
    /// Manage CLI preferences
    Preferences {
        #[command(subcommand)]
        command: Option<preferences::Command>
    },
    /// Test connection to USACO servers
    Ping
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
    let prefs = PreferencesStore::from_file().await?;

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
        Command::Auth { command } => auth::handle(command, client, cred_storage, multi).await?,
        Command::Preferences { command } => preferences::handle(command, &prefs, multi).await?
    }

    Ok(())
}

pub async fn run() {
    let (multi, args) = setup_logging();
    if let Err(err) = run_internal(multi, args).await {
        error!("Unexpected error: {}", err);
    }
}
