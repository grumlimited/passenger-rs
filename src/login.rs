use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::io::{self, Write};
use std::time::Duration;
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
    
    // Create a progress bar for displaying authorization info
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{msg}")
            .unwrap()
    );
    
    // Display authorization instructions
    pb.println("");
    pb.println("=================================================================");
    pb.println("  AUTHORIZATION REQUIRED");
    pb.println("=================================================================");
    pb.println("");
    pb.println(format!("  1. Visit: {}", device_code_response.verification_uri));
    pb.println(format!("  2. Enter code: {}", device_code_response.user_code));
    pb.println("");
    pb.println(format!("  Device code expires in: {} seconds", device_code_response.expires_in));
    pb.println("");
    pb.println("=================================================================");
    pb.println("");
    
    pb.finish_and_clear();
    
    // Wait for user confirmation
    print!("Press ENTER once you have authorized the device...");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    println!();
    
    // Show a spinner while waiting for authorization
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap()
    );
    spinner.set_message("Waiting for GitHub authorization...");
    
    // Start spinner in a separate task
    let spinner_clone = spinner.clone();
    let spinner_handle = tokio::spawn(async move {
        loop {
            spinner_clone.tick();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    
    // Step 2: Poll for access token
    let access_token_response = auth::poll_for_access_token(
        &client,
        &config.github.oauth_token_url,
        &config.github.client_id,
        &device_code_response.device_code,
        device_code_response.interval,
    ).await?;
    
    // Stop spinner
    spinner_handle.abort();
    spinner.finish_with_message("✓ Authorization successful!");
    
    // Display success information
    let success_pb = ProgressBar::new_spinner();
    success_pb.set_style(
        ProgressStyle::default_spinner()
            .template("{msg}")
            .unwrap()
    );
    
    success_pb.println("");
    success_pb.println(format!("Token type: {}", access_token_response.token_type));
    success_pb.println(format!("Scope: {}", access_token_response.scope));
    success_pb.finish_and_clear();
    
    info!("Access token received");

    Ok(())
}
