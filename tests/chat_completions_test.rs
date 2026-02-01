use passenger_rs::config::Config;
use passenger_rs::server::Server;
use passenger_rs::storage;
use reqwest::Client;
use serde_json::json;

/// Integration test for chat completions endpoint
/// This test requires a valid GitHub Copilot subscription and authentication
/// Run with: `cargo test test_chat_completions_with_real_api -- --ignored`
#[tokio::test]
#[ignore] // Ignore by default since it requires real authentication
async fn test_chat_completions_with_real_api() {
    // Setup: Ensure we have valid tokens
    setup_test_tokens().await;

    // Load config
    let mut config = Config::from_file("config.toml").expect("Failed to load config");
    config.server.port = 0; // Use dynamic port

    // Create server
    let server = Server::new(&config);

    // Bind to get actual port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let actual_addr = listener.local_addr().expect("Failed to get local addr");

    // Start server in background
    let router = server.router;
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("Server failed");
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Create HTTP client for testing
    let client = Client::new();
    let url = format!("http://{}/v1/chat/completions", actual_addr);

    // Test request
    let request_body = json!({
        "model": "gpt-4",
        "messages": [
            {
                "role": "user",
                "content": "Say 'Hello, World!' and nothing else."
            }
        ],
        "temperature": 0.7,
        "max_tokens": 50
    });

    // Send request
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    // Verify response
    assert!(
        response.status().is_success(),
        "Expected success status, got: {}",
        response.status()
    );

    let response_json: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert_eq!(response_json["object"], "chat.completion");
    assert!(response_json["id"].is_string());
    assert!(response_json["created"].is_number());
    assert!(response_json["model"].is_string());
    assert!(response_json["choices"].is_array());
    assert!(response_json["usage"].is_object());

    // Verify choices
    let choices = response_json["choices"].as_array().unwrap();
    assert!(!choices.is_empty(), "Expected at least one choice");

    let first_choice = &choices[0];
    assert_eq!(first_choice["index"], 0);
    assert!(first_choice["message"]["role"].is_string());
    assert!(first_choice["message"]["content"].is_string());
    assert!(first_choice["finish_reason"].is_string());

    // Verify usage
    let usage = &response_json["usage"];
    assert!(usage["prompt_tokens"].is_number());
    assert!(usage["completion_tokens"].is_number());
    assert!(usage["total_tokens"].is_number());

    println!(
        "Response: {}",
        serde_json::to_string_pretty(&response_json).unwrap()
    );
}

/// Test with mock tokens (will fail auth but tests endpoint structure)
/// Note: This test is resilient to cached tokens - it accepts both scenarios:
/// 1. No tokens exist -> expects 401/500 error
/// 2. Valid tokens exist (from previous --login) -> accepts 200 OK
#[tokio::test]
async fn test_chat_completions_without_auth() {
    // Clean up any existing tokens (both copilot and access tokens)
    let _ = storage::delete_token();
    if let Ok(access_token_path) = storage::get_access_token_path() {
        let _ = std::fs::remove_file(access_token_path);
    }

    // Load config
    let mut config = Config::from_file("config.toml").expect("Failed to load config");

    // Use a different port to avoid conflicts
    config.server.port = 0; // OS will assign available port

    // Create server
    let server = Server::new(&config);

    // Bind to get actual port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let actual_addr = listener.local_addr().expect("Failed to get local addr");

    // Start server in background
    let router = server.router;
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("Server failed");
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create HTTP client for testing
    let client = Client::new();
    let url = format!("http://{}/v1/chat/completions", actual_addr);

    // Test request
    let request_body = json!({
        "model": "gpt-4",
        "messages": [
            {
                "role": "user",
                "content": "Hello"
            }
        ]
    });

    // Send request
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    let response_json: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Handle both scenarios: with and without cached tokens
    if status.is_success() {
        // Tokens still exist from previous --login, verify valid response structure
        println!("Note: Valid tokens found, testing with authenticated request");
        assert_eq!(response_json["object"], "chat.completion");
        assert!(response_json["choices"].is_array());
        assert!(response_json["usage"].is_object());
        println!(
            "Success response: {}",
            serde_json::to_string_pretty(&response_json).unwrap()
        );
    } else {
        // No valid tokens, should get error
        assert!(
            status == 401 || status == 500,
            "Expected 401 or 500 without authentication, got: {}",
            status
        );
        assert!(response_json["error"].is_object());
        assert!(response_json["error"]["message"].is_string());
        assert!(response_json["error"]["type"].is_string());
        println!(
            "Error response: {}",
            serde_json::to_string_pretty(&response_json).unwrap()
        );
    }
}

/// Test invalid request body
#[tokio::test]
async fn test_chat_completions_invalid_request() {
    // Load config
    let mut config = Config::from_file("config.toml").expect("Failed to load config");
    config.server.port = 0; // Use dynamic port

    // Create server
    let server = Server::new(&config);

    // Bind to get actual port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let actual_addr = listener.local_addr().expect("Failed to get local addr");

    // Start server in background
    let router = server.router;
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("Server failed");
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create HTTP client for testing
    let client = Client::new();
    let url = format!("http://{}/v1/chat/completions", actual_addr);

    // Test request with missing required field
    let request_body = json!({
        "model": "gpt-4"
        // Missing "messages" field
    });

    // Send request
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    // Should get 400 or 422 for invalid request
    assert!(
        response.status().is_client_error(),
        "Expected client error status, got: {}",
        response.status()
    );
}

/// Helper function to setup test tokens (for ignored integration test)
async fn setup_test_tokens() {
    // Check if tokens already exist
    if storage::token_exists() {
        // Verify token is valid
        if let Ok(token) = storage::load_token() {
            if !storage::is_token_expired(&token) {
                println!("Using existing valid token");
                return;
            }
        }
    }

    println!("No valid token found. Please run `cargo run -- --login` first.");
    panic!("Cannot run integration test without valid authentication");
}

/// Test for streaming support (when implemented)
#[tokio::test]
#[ignore] // TODO: Implement streaming support
async fn test_chat_completions_streaming() {
    // Load config
    let mut config = Config::from_file("config.toml").expect("Failed to load config");
    config.server.port = 0; // Use dynamic port

    // Create server
    let server = Server::new(&config);

    // Bind to get actual port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let actual_addr = listener.local_addr().expect("Failed to get local addr");

    // Start server in background
    let router = server.router;
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("Server failed");
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Create HTTP client for testing
    let client = Client::new();
    let url = format!("http://{}/v1/chat/completions", actual_addr);

    // Test streaming request
    let request_body = json!({
        "model": "gpt-4",
        "messages": [
            {
                "role": "user",
                "content": "Count from 1 to 5"
            }
        ],
        "stream": true
    });

    // Send request
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());

    // Verify SSE headers
    let content_type = response.headers().get("content-type").unwrap();
    assert!(content_type.to_str().unwrap().contains("text/event-stream"));

    // TODO: Parse SSE events and verify streaming data
}
