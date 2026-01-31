mod auth;

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting passenger-rs - GitHub Copilot Proxy");

    // Test device code request
    let client = reqwest::Client::new();
    info!("Requesting device code from GitHub...");
    
    let device_code_response = auth::request_device_code(&client, None).await?;
    
    info!("Device code received!");
    info!("Device code: {} (save this for token polling)", device_code_response.device_code);
    info!("Visit: {}", device_code_response.verification_uri);
    info!("Enter code: {}", device_code_response.user_code);
    info!("Device code expires in: {} seconds", device_code_response.expires_in);
    info!("Poll interval: {} seconds", device_code_response.interval);

    Ok(())
}
