mod auth;
mod clap;
mod config;
mod copilot;
mod login;
mod ollama;
mod openai;
mod server;
mod storage;
mod token_manager;

use crate::clap::Args;
use crate::server::Server;
use anyhow::Result;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse_args();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting passenger-rs - GitHub Copilot Proxy");

    args.validate_config_path()?;

    let config = config::Config::from_file(&args.config)?;
    info!("Configuration loaded from {}", args.config);

    if args.execute_command(&config).await? {
        return Ok(());
    }

    args.verify_token_exists()?;

    info!("Starting proxy server...");
    let server = Server::new(&config);

    info!("Server listening on http://{}", server.addr);
    info!("OpenAI chat completions: http://{}/v1/chat/completions", server.addr);
    info!("OpenAI responses:        http://{}/v1/responses", server.addr);

    let listener = tokio::net::TcpListener::bind(&server.addr).await?;
    axum::serve(listener, server.router).await?;

    Ok(())
}
