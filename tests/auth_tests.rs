use passenger_rs::auth::request_device_code;
use reqwest::Client;

#[tokio::test]
async fn test_request_device_code() {
    let client = Client::new();
    let result = request_device_code(&client, None).await;
    
    // This will make a real API call in tests
    // In production you'd mock this
    assert!(result.is_ok(), "Failed to get device code: {:?}", result.err());
    
    let response = result.unwrap();
    assert!(!response.device_code.is_empty());
    assert!(!response.user_code.is_empty());
    assert_eq!(response.verification_uri, "https://github.com/login/device");
    assert!(response.expires_in > 0);
    assert!(response.interval > 0);
}
