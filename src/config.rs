use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub github: GithubConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GithubConfig {
    pub device_code_url: String,
    pub oauth_token_url: String,
    pub client_id: String,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file(path: &str) -> Result<Self> {
        let contents =
            fs::read_to_string(path).context(format!("Failed to read config file: {}", path))?;

        let config: Config =
            toml::from_str(&contents).context("Failed to parse config file as TOML")?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_file() {
        let config = Config::from_file("config.toml");
        assert!(config.is_ok(), "Failed to load config: {:?}", config.err());

        let config = config.unwrap();
        assert_eq!(
            config.github.device_code_url,
            "https://github.com/login/device/code"
        );
        assert_eq!(
            config.github.oauth_token_url,
            "https://github.com/login/oauth/access_token"
        );
        assert_eq!(config.github.client_id, "Iv1.b507a08c87ecfe98");
    }
}
