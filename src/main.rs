mod auth;
mod config;
mod login;
mod server;
mod storage;
mod token_manager;

use anyhow::Result;
use clap::Parser;
use reqwest::Client;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// GitHub Copilot proxy server
#[derive(Parser, Debug)]
#[command(name = "passenger-rs")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    /// Perform GitHub OAuth device flow login
    #[arg(long)]
    login: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting passenger-rs - GitHub Copilot Proxy");

    // Load configuration
    let config = config::Config::from_file(&args.config)?;
    info!("Configuration loaded from {}", args.config);

    // Handle login if requested
    if args.login {
        return login::login(&config).await;
    }

    // Check if we have a valid token
    if !storage::token_exists() {
        info!("No authentication token found.");
        info!("Please run with --login to authenticate with GitHub");
        return Ok(());
    }

    // Start proxy server
    info!("Starting OpenAI-compatible proxy server...");
    
    let client = Client::new();
    let state = server::AppState {
        config: config.clone(),
        client,
    };

    let app = server::create_router(state);
    let addr = format!("{}:{}", config.server.host, config.server.port);
    
    info!("Server listening on http://{}", addr);
    info!("OpenAI API endpoint: http://{}/v1/chat/completions", addr);
    info!("Models endpoint: http://{}/v1/models", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
