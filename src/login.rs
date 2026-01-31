use anyhow::Result;
use reqwest::Client;
use std::io::{self, Write};
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
    println!();
    println!("=================================================================");
    println!("  AUTHORIZATION REQUIRED");
    println!("=================================================================");
    println!();
    println!("  1. Visit: {}", device_code_response.verification_uri);
    println!("  2. Enter code: {}", device_code_response.user_code);
    println!();
    println!("  Device code expires in: {} seconds", device_code_response.expires_in);
    println!();
    println!("=================================================================");
    println!();
    print!("Once you have authorized the device, press ENTER to continue...");
    io::stdout().flush()?;
    
    // Wait for user to press Enter
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    // Step 2: Poll for access token
    info!("Checking authorization status...");
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
