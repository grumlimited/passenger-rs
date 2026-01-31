mod auth;
mod config;

use anyhow::Result;
use clap::Parser;
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

    // Test device code request
    let client = reqwest::Client::new();
    info!("Requesting device code from GitHub...");
    
    let device_code_response = auth::request_device_code(
        &client,
        &config.github.device_code_url,
        &config.github.client_id,
    ).await?;
    
    info!("Device code received!");
    info!("Device code: {} (save this for token polling)", device_code_response.device_code);
    info!("Visit: {}", device_code_response.verification_uri);
    info!("Enter code: {}", device_code_response.user_code);
    info!("Device code expires in: {} seconds", device_code_response.expires_in);
    info!("Poll interval: {} seconds", device_code_response.interval);

    Ok(())
}
