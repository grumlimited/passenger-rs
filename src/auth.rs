use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Response from GitHub device code request
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Request body for device code
#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
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
/// let client = reqwest::Client::new();
/// let response = request_device_code(
///     &client,
///     "https://github.com/login/device/code",
///     "Iv1.b507a08c87ecfe98"
/// ).await?;
/// println!("Visit: {}", response.verification_uri);
/// println!("Enter code: {}", response.user_code);
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
}

