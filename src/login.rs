use anyhow::Result;
use reqwest::Client;
use tracing::info;

use crate::auth;
use crate::config::Config;

/// Perform GitHub OAuth device flow login
pub async fn login(config: &Config) -> Result<()> {
    let client = Client::new();
    
    // Step 1: Request device code
    info!("Requesting device code from GitHub...");
    let device_code_response = auth::request_device_code(
        &client,
        &config.github.device_code_url,
        &config.github.client_id,
    ).await?;
    
    info!("Device code received!");
    info!("Visit: {}", device_code_response.verification_uri);
    info!("Enter code: {}", device_code_response.user_code);
    info!("Device code expires in: {} seconds", device_code_response.expires_in);
    info!("Poll interval: {} seconds", device_code_response.interval);
    
    // Step 2: Wait for user to authorize device
    info!("Waiting for authorization...");
    let access_token_response = auth::poll_for_access_token(
        &client,
        &config.github.oauth_token_url,
        &config.github.client_id,
        &device_code_response.device_code,
        device_code_response.interval,
    ).await?;
    
    info!("Authorization successful!");
    info!("Access token: {}", access_token_response.access_token);
    info!("Token type: {}", access_token_response.token_type);
    info!("Scope: {}", access_token_response.scope);

    Ok(())
}
