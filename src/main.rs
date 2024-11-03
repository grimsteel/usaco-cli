mod credential_storage;
mod http_client;
mod preferences;
mod cli;

#[tokio::main]
async fn main() {
    cli::run().await;
}
