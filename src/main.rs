mod credential_storage;
mod http_client;
mod preferences;
mod cli;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    cli::run().await
}
