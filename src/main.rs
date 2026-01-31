mod auth;
mod config;
mod login;

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

    // Default: start proxy server (to be implemented)
    info!("Proxy server mode - not yet implemented");
    info!("Use --login to authenticate with GitHub");

    Ok(())
}
