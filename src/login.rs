use crate::auth;
use crate::auth::DeviceCodeResponse;
use crate::config::Config;
use crate::storage;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
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
        ct.clone(),
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

    println!("Press ENTER once you have authorized the device...");
    io::stdout().flush()?;

    let spinner_clone = spinner.clone();
    let timeout_duration = Duration::from_secs(device_code_response.expires_in);
    // For testing: let timeout_duration = Duration::from_secs(3);

    let (tx, _rx) = mpsc::channel::<()>(1);
    let ct_clone = cancellation_token.clone();

    // Spawn countdown task
    tokio::spawn(async move {
        let start = tokio::time::Instant::now();

        loop {
            if ct_clone.is_cancelled() {
                spinner_clone.finish_with_message("✓ Authorization successful!");
                return;
            }

            let elapsed = start.elapsed();
            if elapsed >= timeout_duration {
                spinner_clone.finish_with_message("✗ Timeout expired. Exiting...");
                ct_clone.cancel();
                let _ = tx.send(()).await; // Signal main loop to exit
                return;
            }

            let remaining = (timeout_duration - elapsed).as_secs();
            spinner_clone.set_message(format!(
                "Waiting for device authorization ({} seconds remaining)",
                remaining
            ));

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    // Non-blocking keyboard input check using crossterm
    let check_interval = Duration::from_millis(100);
    loop {
        // Check if timeout occurred
        if cancellation_token.is_cancelled() {
            spinner.finish_and_clear();
            return Err(anyhow::anyhow!(
                "Authentication timeout expired. Please try again."
            ));
        }

        // Check if Enter key was pressed (non-blocking)
        if event::poll(check_interval)?
            && let Event::Key(key_event) = event::read()?
            && key_event.code == KeyCode::Enter
        {
            // User pressed Enter, continue to polling
            spinner.finish_and_clear();
            return Ok(());
        }
    }
}
