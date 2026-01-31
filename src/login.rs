use anyhow::Result;
use reqwest::Client;
use tracing::info;

use crate::auth;
use crate::config::Config;

/// Perform GitHub OAuth device flow login
pub async fn login(config: &Config) -> Result<()> {
    let client = Client::new();
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
