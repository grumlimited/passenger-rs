use crate::auth::{AccessTokenResponse, CopilotTokenResponse};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Get the token storage directory path (~/.config/passenger-rs/)
pub fn get_storage_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Could not determine home directory")?;

    let config_dir = PathBuf::from(home).join(".config").join("passenger-rs");
    Ok(config_dir)
}

pub fn get_access_token_path() -> Result<PathBuf> {
    Ok(get_storage_dir()?.join("access_token.json"))
}

/// Get the token file path (~/.config/passenger-rs/token.json)
pub fn get_token_path() -> Result<PathBuf> {
    Ok(get_storage_dir()?.join("token.json"))
}

/// Save a Copilot token to disk (with optional custom path)
pub fn save_token_to_path(token: &CopilotTokenResponse, custom_path: Option<&Path>) -> Result<()> {
    let token_path = match custom_path {
        Some(path) => {
            // Verify custom path is valid
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return Err(anyhow::anyhow!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    ));
                }
            }
            path.to_path_buf()
        }
        None => {
            let storage_dir = get_storage_dir()?;
            // Create the directory if it doesn't exist
            fs::create_dir_all(&storage_dir).context("Failed to create storage directory")?;
            get_token_path()?
        }
    };

    let token_json = serde_json::to_string_pretty(token).context("Failed to serialize token")?;
    fs::write(&token_path, token_json).context("Failed to write token to disk")?;

    Ok(())
}

/// Save a Copilot token to disk (default path)
pub fn save_token(token: &CopilotTokenResponse) -> Result<()> {
    save_token_to_path(token, None)
}

pub fn save_access_token_to_path(
    token: &AccessTokenResponse,
    custom_path: Option<&Path>,
) -> Result<()> {
    let token_path = match custom_path {
        Some(path) => {
            // Verify custom path is valid
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return Err(anyhow::anyhow!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    ));
                }
            }
            path.to_path_buf()
        }
        None => {
            let storage_dir = get_storage_dir()?;
            // Create the directory if it doesn't exist
            fs::create_dir_all(&storage_dir).context("Failed to create storage directory")?;
            get_access_token_path()?
        }
    };

    let token_json =
        serde_json::to_string_pretty(token).context("Failed to serialize access token")?;
    fs::write(&token_path, token_json).context("Failed to write access token to disk")?;

    Ok(())
}

pub fn save_access_token(token: &AccessTokenResponse) -> Result<()> {
    save_access_token_to_path(token, None)
}

/// Load a Copilot token from disk (with optional custom path)
pub fn load_token_from_path(custom_path: Option<&Path>) -> Result<CopilotTokenResponse> {
    let token_path = match custom_path {
        Some(path) => {
            if !path.exists() {
                return Err(anyhow::anyhow!(
                    "Copilot token file does not exist: {}",
                    path.display()
                ));
            }
            path.to_path_buf()
        }
        None => get_token_path()?,
    };

    let token_json = fs::read_to_string(&token_path).context(format!(
        "Failed to read token from {}",
        token_path.display()
    ))?;

    let token: CopilotTokenResponse =
        serde_json::from_str(&token_json).context("Failed to deserialize token")?;

    Ok(token)
}

/// Load a Copilot token from disk (default path)
pub fn load_token() -> Result<CopilotTokenResponse> {
    load_token_from_path(None)
}

pub fn load_access_token_from_path(
    custom_path: Option<&Path>,
) -> Result<Option<AccessTokenResponse>> {
    let token_path = match custom_path {
        Some(path) => {
            if !path.exists() {
                return Err(anyhow::anyhow!(
                    "Access token file does not exist: {}",
                    path.display()
                ));
            }
            path.to_path_buf()
        }
        None => get_access_token_path()?,
    };

    match fs::read_to_string(&token_path) {
        Ok(token_json) => {
            let token: AccessTokenResponse =
                serde_json::from_str(&token_json).context("Failed to deserialize token")?;
            Ok(Some(token))
        }
        Err(_) => Ok(None),
    }
}

pub fn load_access_token() -> Result<Option<AccessTokenResponse>> {
    load_access_token_from_path(None)
}

/// Check if a token exists on disk
pub fn token_exists() -> bool {
    get_token_path().map(|path| path.exists()).unwrap_or(false)
}

/// Check if a token exists at custom path
#[allow(unused)]
pub fn token_exists_at_path(path: &Path) -> bool {
    path.exists()
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
#[allow(unused)]
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
