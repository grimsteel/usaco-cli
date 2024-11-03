mod credential_storage;
mod http_client;
mod cli;

#[tokio::main]
async fn main() {
    cli::run().await;
}
