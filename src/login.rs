use crate::auth;
use crate::auth::DeviceCodeResponse;
use crate::config::Config;
use crate::storage;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::io::{self, Write};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Perform GitHub OAuth device flow login
pub async fn login(config: &Config) -> Result<()> {
    let client = Client::new();

    // Step 1: Request device code
    info!("Requesting device code from GitHub...");
    let device_code_response = auth::request_device_code(
        &client,
        &config.github.device_code_url,
        &config.github.client_id,
    )
    .await?;

    info!("Device code received!");

    let ct = CancellationToken::new();

    spinner(&device_code_response, ct.clone()).await?;

    // Step 2: Poll for access token
    let access_token_response = auth::poll_for_access_token(
        &client,
        &config.github.oauth_token_url,
        &config.github.client_id,
        &device_code_response.device_code,
        device_code_response.interval,
    )
    .await?;

    info!("Access token received");
    storage::save_access_token(&access_token_response)?;

    // Stop spinner
    ct.cancel();

    // Step 3: Get Copilot token
    info!("Requesting Copilot token...");
    let copilot_token_response = auth::get_copilot_token(
        &client,
        &config.github.copilot_token_url,
        &access_token_response.access_token,
    )
    .await?;

    // Save the token to disk
    storage::save_token(&copilot_token_response)?;
    let token_path = storage::get_token_path()?;

    // Display success information
    let success_pb = ProgressBar::new_spinner();
    success_pb.set_style(ProgressStyle::default_spinner().template("{msg}")?);

    success_pb.println("");
    success_pb.println("✓ Login successful!");
    success_pb.println("");
    success_pb.println(format!("Copilot token: {}", copilot_token_response.token));
    success_pb.println(format!(
        "Expires at: {} (Unix timestamp)",
        copilot_token_response.expires_at
    ));
    success_pb.println(format!(
        "Refresh in: {} seconds",
        copilot_token_response.refresh_in
    ));
    success_pb.println(format!("Token saved to: {}", token_path.display()));
    success_pb.println("");
    success_pb.finish_and_clear();

    info!("Copilot token received and ready to use");

    Ok(())
}

pub async fn spinner(
    device_code_response: &DeviceCodeResponse,
    cancellation_token: CancellationToken,
) -> Result<()> {
    // Create a progress bar for displaying authorization info
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner().template("{msg}")?);

    // Display authorization instructions
    pb.println("");
    pb.println("=================================================================");
    pb.println("  AUTHORIZATION REQUIRED");
    pb.println("=================================================================");
    pb.println("");
    pb.println(format!(
        "  1. Visit: {}",
        device_code_response.verification_uri
    ));
    pb.println(format!(
        "  2. Enter code: {}",
        device_code_response.user_code
    ));
    pb.println("");
    pb.println("=================================================================");
    pb.println("");

    pb.finish_and_clear();

    // Show a spinner while waiting for authorization
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")?,
    );
    spinner.enable_steady_tick(Duration::from_millis(100));

    // Wait for user confirmation
    println!("Press ENTER once you have authorized the device...");

    let spinner_clone = spinner.clone();

    let (tx, mut rx) = mpsc::channel::<f32>(1);
    let _ = tx.send(device_code_response.expires_in as f32).await;
    tokio::spawn(async move {
        while let Some(x) = rx.recv().await {
            if cancellation_token.is_cancelled() {
                spinner_clone.finish_with_message("✓ Authorization successful!");
                break;
            }

            spinner_clone.set_message(format!("Wait for device authorisation ({} seconds)", x));
            tokio::time::sleep(Duration::from_secs_f32(1_f32)).await;
            let _ = tx.send(x - 1_f32).await;
        }
    });

    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    println!();

    // Stop the spinner cleanly
    spinner.finish_and_clear();

    Ok(())
}
