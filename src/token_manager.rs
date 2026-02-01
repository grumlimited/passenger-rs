use crate::auth::{self, AccessTokenResponse, CopilotTokenResponse};
use crate::config::Config;
use crate::storage;
use anyhow::{bail, Context, Result};
use reqwest::Client;
use tracing::{info, warn};

/// Get a valid Copilot token, either from cache or by refreshing
pub async fn get_valid_token(
    config: &Config,
    client: &Client,
    // github_access_token: Option<&str>,
) -> Result<CopilotTokenResponse> {
    // Try to load token from disk
    if storage::token_exists() {
        match storage::load_token() {
            Ok(token) => {
                if !storage::is_token_expired(&token) {
                    info!("Using cached Copilot token");
                    return Ok(token);
                } else {
                    warn!("Cached token is expired, refreshing...");
                }
            }
            Err(e) => {
                warn!("Failed to load cached token: {}", e);
            }
        }
    }

    // If we get here, we need to refresh the token
    let github_access_token = storage::load_access_token()?;
    refresh_token(config, client, github_access_token).await
}

/// Refresh the Copilot token using a GitHub access token
async fn refresh_token(
    config: &Config,
    client: &Client,
    github_access_token: Option<AccessTokenResponse>,
) -> Result<CopilotTokenResponse> {
    let access_token = match github_access_token {
        Some(token) => token.access_token.to_string(),
        None => {
            bail!("No GitHub access token available. Please run with --login to authenticate.");
        }
    };

    info!("Refreshing Copilot token...");
    let copilot_token =
        auth::get_copilot_token(client, &config.github.copilot_token_url, &access_token)
            .await
            .context("Failed to refresh Copilot token")?;

    // Save the new token
    storage::save_token(&copilot_token).context("Failed to save refreshed token")?;

    info!("Copilot token refreshed and saved");
    Ok(copilot_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_valid_token_no_cache() {
        // This test requires a valid GitHub access token
        // In a real scenario, we'd mock the HTTP calls

        // Clean up any existing token
        let _ = storage::delete_token();

        let config = Config::from_file("config.toml").unwrap();
        let client = Client::new();

        // Without access token, should fail
        let result = get_valid_token(&config, &client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_token_no_access_token() {
        let config = Config::from_file("config.toml").unwrap();
        let client = Client::new();

        let result = refresh_token(&config, &client, None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No GitHub access token"));
    }
}
