mod auth;
mod clap;
mod config;
mod login;
mod server;
mod server_chat_completion;
mod server_list_models;
mod server_ollama_chat;
mod storage;
mod token_manager;

use crate::clap::Args;
use crate::server::Server;
use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse_args();

    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting passenger-rs - GitHub Copilot Proxy");

    // Validate configuration file exists
    args.validate_config_path()?;

    // Load configuration
    let config = config::Config::from_file(&args.config)?;
    info!("Configuration loaded from {}", args.config);

    // Execute any commands (login, refresh-token, etc.)
    // If a command was executed, exit early
    if args.execute_command(&config).await? {
        return Ok(());
    }

    // Verify token exists before starting server
    args.verify_token_exists()?;

    // Start proxy server
    info!("Starting OpenAI-compatible proxy server...");
    let server = Server::new(&config);

    info!("Server listening on http://{}", server.addr);
    info!(
        "OpenAI API endpoint: http://{}/v1/chat/completions",
        server.addr
    );
    info!(
        "Ollama API endpoint: http://{}/v1/api/chat",
        server.addr
    );
    info!("Models endpoint: http://{}/v1/models", server.addr);

    let listener = tokio::net::TcpListener::bind(&server.addr).await?;
    axum::serve(listener, server.router).await?;

    Ok(())
}
