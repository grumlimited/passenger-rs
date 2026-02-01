use crate::auth::CopilotTokenResponse;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get the token storage directory path (~/.config/passenger-rs/)
pub fn get_storage_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Could not determine home directory")?;

    let config_dir = PathBuf::from(home).join(".config").join("passenger-rs");
    Ok(config_dir)
}

/// Get the token file path (~/.config/passenger-rs/token.json)
pub fn get_token_path() -> Result<PathBuf> {
    Ok(get_storage_dir()?.join("token.json"))
}

/// Save a Copilot token to disk
pub fn save_token(token: &CopilotTokenResponse) -> Result<()> {
    let storage_dir = get_storage_dir()?;

    // Create the directory if it doesn't exist
    fs::create_dir_all(&storage_dir).context("Failed to create storage directory")?;

    let token_path = get_token_path()?;
    let token_json = serde_json::to_string_pretty(token).context("Failed to serialize token")?;

    fs::write(&token_path, token_json).context("Failed to write token to disk")?;

    Ok(())
}

/// Load a Copilot token from disk
pub fn load_token() -> Result<CopilotTokenResponse> {
    let token_path = get_token_path()?;

    let token_json = fs::read_to_string(&token_path).context("Failed to read token from disk")?;

    let token: CopilotTokenResponse =
        serde_json::from_str(&token_json).context("Failed to deserialize token")?;

    Ok(token)
}

/// Check if a token exists on disk
pub fn token_exists() -> bool {
    get_token_path().map(|path| path.exists()).unwrap_or(false)
}

/// Check if a token is expired (returns true if expired or within 60 seconds of expiring)
pub fn is_token_expired(token: &CopilotTokenResponse) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Consider expired if we're within 60 seconds of expiration (buffer)
    token.expires_at <= now + 60
}

/// Delete the stored token
pub fn delete_token() -> Result<()> {
    let token_path = get_token_path()?;

    if token_path.exists() {
        fs::remove_file(&token_path).context("Failed to delete token file")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_get_storage_dir() {
        let dir = get_storage_dir().unwrap();
        assert!(dir.ends_with(".config/passenger-rs"));
    }

    #[test]
    fn test_get_token_path() {
        let path = get_token_path().unwrap();
        assert!(path.ends_with(".config/passenger-rs/token.json"));
    }

    #[test]
    fn test_is_token_expired() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Token expired 1 hour ago
        let expired_token = CopilotTokenResponse {
            token: "test".to_string(),
            expires_at: now - 3600,
            refresh_in: 0,
        };
        assert!(is_token_expired(&expired_token));

        // Token expires in 30 seconds (within buffer, should be considered expired)
        let almost_expired_token = CopilotTokenResponse {
            token: "test".to_string(),
            expires_at: now + 30,
            refresh_in: 0,
        };
        assert!(is_token_expired(&almost_expired_token));

        // Token expires in 10 minutes (valid)
        let valid_token = CopilotTokenResponse {
            token: "test".to_string(),
            expires_at: now + 600,
            refresh_in: 0,
        };
        assert!(!is_token_expired(&valid_token));
    }
}
