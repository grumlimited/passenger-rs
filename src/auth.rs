use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Response from GitHub device code request
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Response from GitHub access token request
#[derive(Debug, Deserialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    #[allow(dead_code)]
    pub token_type: String,
    #[allow(dead_code)]
    pub scope: String,
}

/// Response from Copilot token request
#[derive(Debug, Serialize, Deserialize)]
pub struct CopilotTokenResponse {
    pub token: String,
    pub expires_at: u64,
    pub refresh_in: u64,
}

/// Error response from GitHub access token polling
#[derive(Debug, Deserialize)]
pub struct AccessTokenError {
    pub error: String,
    pub error_description: String,
    #[allow(dead_code)]
    pub error_uri: String,
}

/// Request body for device code
#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
}

/// Request body for access token
#[derive(Debug, Serialize)]
struct AccessTokenRequest {
    client_id: String,
    device_code: String,
    grant_type: String,
}

/// Request GitHub device code for OAuth flow
///
/// # Arguments
/// * `client` - HTTP client to use for the request
/// * `device_code_url` - GitHub device code endpoint URL
/// * `client_id` - GitHub OAuth client ID
///
/// # Example
/// ```no_run
/// use passenger_rs::auth::request_device_code;
/// use reqwest::Client;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = Client::new();
///     let response = request_device_code(
///         &client,
///         "https://github.com/login/device/code",
///         "Iv1.b507a08c87ecfe98"
///     ).await?;
///     println!("Visit: {}", response.verification_uri);
///     println!("Enter code: {}", response.user_code);
///     Ok(())
/// }
/// ```
pub async fn request_device_code(
    client: &Client,
    device_code_url: &str,
    client_id: &str,
) -> Result<DeviceCodeResponse> {
    let request_body = DeviceCodeRequest {
        client_id: client_id.to_string(),
        scope: "read:user".to_string(),
    };

    let response = client
        .post(device_code_url)
        .header("accept", "application/json")
        .header("editor-version", "Neovim/0.6.1")
        .header("editor-plugin-version", "copilot.vim/1.16.0")
        .header("content-type", "application/json")
        .header("user-agent", "GithubCopilot/1.155.0")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send device code request")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Device code request failed with status {}: {}", status, error_text);
    }

    response
        .json::<DeviceCodeResponse>()
        .await
        .context("Failed to parse device code response")
}

/// Poll GitHub for access token after user authorization
///
/// This function polls the GitHub OAuth token endpoint until:
/// - The user authorizes the device (success)
/// - The device code expires (failure)
/// - An error occurs (failure)
///
/// # Arguments
/// * `client` - HTTP client to use for requests
/// * `oauth_token_url` - GitHub OAuth token endpoint URL
/// * `client_id` - GitHub OAuth client ID
/// * `device_code` - Device code from `request_device_code()`
/// * `interval` - Seconds to wait between polls (from `request_device_code()`)
///
/// # Returns
/// Access token on success
///
/// # Example
/// ```no_run
/// use passenger_rs::auth::{request_device_code, poll_for_access_token};
/// use reqwest::Client;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = Client::new();
///     let device_resp = request_device_code(
///         &client,
///         "https://github.com/login/device/code",
///         "Iv1.b507a08c87ecfe98"
///     ).await?;
///     
///     println!("Visit: {} and enter: {}", device_resp.verification_uri, device_resp.user_code);
///     
///     let token = poll_for_access_token(
///         &client,
///         "https://github.com/login/oauth/access_token",
///         "Iv1.b507a08c87ecfe98",
///         &device_resp.device_code,
///         device_resp.interval,
///     ).await?;
///     
///     println!("Access token: {}", token.access_token);
///     Ok(())
/// }
/// ```
pub async fn poll_for_access_token(
    client: &Client,
    oauth_token_url: &str,
    client_id: &str,
    device_code: &str,
    interval: u64,
) -> Result<AccessTokenResponse> {
    let request_body = AccessTokenRequest {
        client_id: client_id.to_string(),
        device_code: device_code.to_string(),
        grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
    };

    loop {
        info!("Polling for access token...");
        
        let response = client
            .post(oauth_token_url)
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .context("Failed to send access token request")?;

        let response_text = response.text().await.context("Failed to read response body")?;
        
        // Try to parse as error response first (has "error" field)
        if let Ok(error_response) = serde_json::from_str::<AccessTokenError>(&response_text) {
            match error_response.error.as_str() {
                "authorization_pending" => {
                    debug!("Waiting for user to authorize device...");
                    sleep(Duration::from_secs(interval)).await;
                    continue;
                }
                "slow_down" => {
                    warn!("Rate limited, slowing down polling...");
                    sleep(Duration::from_secs(interval + 5)).await;
                    continue;
                }
                "expired_token" => {
                    anyhow::bail!("Device code expired. Please restart the login process.");
                }
                "access_denied" => {
                    anyhow::bail!("User denied access.");
                }
                _ => {
                    anyhow::bail!(
                        "Access token request failed: {} - {}",
                        error_response.error,
                        error_response.error_description
                    );
                }
            }
        }

        // Try to parse as success response
        let token_response: AccessTokenResponse = serde_json::from_str(&response_text)
            .context("Failed to parse access token response")?;

        info!("Access token received successfully");
        return Ok(token_response);
    }
}

/// Retrieve Copilot-specific token from GitHub access token
///
/// This function exchanges a GitHub OAuth access token for a Copilot-specific token
/// that can be used with the Copilot API endpoints.
///
/// # Arguments
/// * `client` - HTTP client to use for the request
/// * `copilot_token_url` - GitHub Copilot token endpoint URL
/// * `access_token` - GitHub OAuth access token from `poll_for_access_token()`
///
/// # Returns
/// Copilot token response with token, expiration, and refresh time
///
/// # Example
/// ```no_run
/// use passenger_rs::auth::{request_device_code, poll_for_access_token, get_copilot_token};
/// use reqwest::Client;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = Client::new();
///     
///     // Get device code and access token first...
///     let device_resp = request_device_code(
///         &client,
///         "https://github.com/login/device/code",
///         "Iv1.b507a08c87ecfe98"
///     ).await?;
///     
///     let access_token_resp = poll_for_access_token(
///         &client,
///         "https://github.com/login/oauth/access_token",
///         "Iv1.b507a08c87ecfe98",
///         &device_resp.device_code,
///         device_resp.interval,
///     ).await?;
///     
///     // Get Copilot token
///     let copilot_token = get_copilot_token(
///         &client,
///         "https://api.github.com/copilot_internal/v2/token",
///         &access_token_resp.access_token,
///     ).await?;
///     
///     println!("Copilot token: {}", copilot_token.token);
///     Ok(())
/// }
/// ```
pub async fn get_copilot_token(
    client: &Client,
    copilot_token_url: &str,
    access_token: &str,
) -> Result<CopilotTokenResponse> {
    let response = client
        .get(copilot_token_url)
        .header("authorization", format!("token {}", access_token))
        .header("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0")
        .header("accept", "application/json")
        .header("accept-language", "en-US,en;q=0.9")
        .send()
        .await
        .context("Failed to send Copilot token request")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Copilot token request failed with status {}: {}",
            status,
            error_text
        );
    }

    let copilot_token_response = response
        .json::<CopilotTokenResponse>()
        .await
        .context("Failed to parse Copilot token response")?;

    info!("Copilot token received successfully");
    Ok(copilot_token_response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, header, body_json};
    use serde_json::json;

    #[tokio::test]
    async fn test_request_device_code_success() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock response
        let mock_response = json!({
            "device_code": "test_device_code_12345",
            "user_code": "ABCD-1234",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 899,
            "interval": 5
        });

        // Setup mock expectations
        Mock::given(method("POST"))
            .and(path("/device/code"))
            .and(header("accept", "application/json"))
            .and(header("editor-version", "Neovim/0.6.1"))
            .and(header("editor-plugin-version", "copilot.vim/1.16.0"))
            .and(header("content-type", "application/json"))
            .and(header("user-agent", "GithubCopilot/1.155.0"))
            .and(body_json(json!({
                "client_id": "Iv1.b507a08c87ecfe98",
                "scope": "read:user"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Make request
        let client = Client::new();
        let url = format!("{}/device/code", mock_server.uri());
        let result = request_device_code(&client, &url, "Iv1.b507a08c87ecfe98").await;

        // Assertions
        assert!(result.is_ok(), "Request should succeed");
        let response = result.unwrap();
        assert_eq!(response.device_code, "test_device_code_12345");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.verification_uri, "https://github.com/login/device");
        assert_eq!(response.expires_in, 899);
        assert_eq!(response.interval, 5);
    }

    #[tokio::test]
    async fn test_request_device_code_error_response() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock error response
        Mock::given(method("POST"))
            .and(path("/device/code"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        // Make request
        let client = Client::new();
        let url = format!("{}/device/code", mock_server.uri());
        let result = request_device_code(&client, &url, "Iv1.b507a08c87ecfe98").await;

        // Assertions
        assert!(result.is_err(), "Request should fail with 401");
        let error = result.unwrap_err();
        assert!(error.to_string().contains("401"));
    }

    #[tokio::test]
    async fn test_poll_for_access_token_success() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock response - success on first poll
        let mock_response = json!({
            "access_token": "gho_test_access_token_12345",
            "token_type": "bearer",
            "scope": "read:user"
        });

        Mock::given(method("POST"))
            .and(path("/oauth/access_token"))
            .and(header("accept", "application/json"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Make request
        let client = Client::new();
        let url = format!("{}/oauth/access_token", mock_server.uri());
        let result = poll_for_access_token(
            &client,
            &url,
            "Iv1.b507a08c87ecfe98",
            "test_device_code",
            1, // Short interval for testing
        ).await;

        // Assertions
        assert!(result.is_ok(), "Request should succeed");
        let response = result.unwrap();
        assert_eq!(response.access_token, "gho_test_access_token_12345");
        assert_eq!(response.token_type, "bearer");
        assert_eq!(response.scope, "read:user");
    }

    #[tokio::test]
    async fn test_poll_for_access_token_expired() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock response - expired token
        let mock_response = json!({
            "error": "expired_token",
            "error_description": "The device code has expired",
            "error_uri": "https://docs.github.com/developers/apps"
        });

        Mock::given(method("POST"))
            .and(path("/oauth/access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Make request
        let client = Client::new();
        let url = format!("{}/oauth/access_token", mock_server.uri());
        let result = poll_for_access_token(
            &client,
            &url,
            "Iv1.b507a08c87ecfe98",
            "test_device_code",
            1,
        ).await;

        // Assertions
        assert!(result.is_err(), "Request should fail with expired token");
        let error = result.unwrap_err();
        let error_msg = error.to_string();
        assert!(
            error_msg.contains("Device code expired") || error_msg.contains("expired"),
            "Expected error about expired token, got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn test_get_copilot_token_success() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock response
        let mock_response = json!({
            "token": "copilot_test_token_abcdef123456",
            "expires_at": 1735689600,
            "refresh_in": 1500
        });

        Mock::given(method("GET"))
            .and(path("/copilot_internal/v2/token"))
            .and(header("authorization", "token gho_test_access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Make request
        let client = Client::new();
        let url = format!("{}/copilot_internal/v2/token", mock_server.uri());
        let result = get_copilot_token(
            &client,
            &url,
            "gho_test_access_token",
        ).await;

        // Assertions
        assert!(result.is_ok(), "Request should succeed");
        let response = result.unwrap();
        assert_eq!(response.token, "copilot_test_token_abcdef123456");
        assert_eq!(response.expires_at, 1735689600);
        assert_eq!(response.refresh_in, 1500);
    }

    #[tokio::test]
    async fn test_get_copilot_token_unauthorized() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Setup mock error response
        Mock::given(method("GET"))
            .and(path("/copilot_internal/v2/token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        // Make request
        let client = Client::new();
        let url = format!("{}/copilot_internal/v2/token", mock_server.uri());
        let result = get_copilot_token(
            &client,
            &url,
            "invalid_token",
        ).await;

        // Assertions
        assert!(result.is_err(), "Request should fail with 401");
        let error = result.unwrap_err();
        assert!(error.to_string().contains("401"));
    }
}

