use crate::auth;
use crate::config::Config;
use crate::login;
use crate::storage;
use anyhow::Result;
use clap::Parser;
use std::path::Path;
use tracing::info;

/// Command-line arguments for passenger-rs
#[derive(Parser, Debug)]
#[command(name = "passenger-rs")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,

    /// Perform GitHub OAuth device flow login
    #[arg(long)]
    pub login: bool,

    /// Refresh Copilot token using existing access token
    #[arg(long)]
    pub refresh_token: bool,

    /// Path to the access token file (defaults to ~/.config/passenger-rs/access_token.json)
    #[arg(long)]
    pub access_token_path: Option<String>,

    /// Path to the Copilot token file (defaults to ~/.config/passenger-rs/token.json)
    #[arg(long)]
    pub copilot_token_path: Option<String>,

    /// Display version information
    #[arg(long)]
    pub version: bool,
}

impl Args {
    /// Parse command-line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Validate that the config file exists
    pub fn validate_config_path(&self) -> Result<()> {
        let config_path = Path::new(&self.config);

        if !config_path.exists() {
            return Err(anyhow::anyhow!(
                "Configuration file does not exist: {}\n\
                 Please create a config.toml file or specify a valid path with --config",
                self.config
            ));
        }

        if !config_path.is_file() {
            return Err(anyhow::anyhow!(
                "Configuration path is not a file: {}",
                self.config
            ));
        }

        Ok(())
    }

    /// Execute the appropriate command based on parsed arguments
    /// Returns Ok(true) if a command was executed, Ok(false) if server should start
    pub async fn execute_command(&self, config: &Config) -> Result<bool> {
        // Handle version if requested
        if self.version {
            self.display_version();
            return Ok(true);
        }

        // Handle login if requested
        if self.login {
            self.handle_login(config).await?;
            return Ok(true);
        }

        // Handle token refresh if requested
        if self.refresh_token {
            self.handle_refresh_token(config).await?;
            return Ok(true);
        }

        // No command executed, continue to server startup
        Ok(false)
    }

    /// Display the version information
    fn display_version(&self) {
        println!("passenger-rs #VERSION");
    }

    /// Handle the --login command
    async fn handle_login(&self, config: &Config) -> Result<()> {
        // For login, we save to custom paths if specified
        let result = login::login(config).await;

        // If custom paths are specified, move the tokens after login
        if result.is_ok() {
            if let Some(ref access_token_path) = self.access_token_path {
                if let Ok(Some(token)) = storage::load_access_token() {
                    storage::save_access_token_to_path(&token, Some(Path::new(access_token_path)))?;
                    info!("Access token saved to custom path: {}", access_token_path);
                }
            }
            if let Some(ref copilot_token_path) = self.copilot_token_path {
                if let Ok(token) = storage::load_token() {
                    storage::save_token_to_path(&token, Some(Path::new(copilot_token_path)))?;
                    info!("Copilot token saved to custom path: {}", copilot_token_path);
                }
            }
        }

        result
    }

    /// Handle the --refresh-token command
    async fn handle_refresh_token(&self, config: &Config) -> Result<()> {
        info!("Refreshing Copilot token...");

        // Determine which path to use for access token
        let access_token_path = self.access_token_path.as_deref().map(Path::new);

        // Check if access token exists
        match storage::load_access_token_from_path(access_token_path)? {
            Some(access_token_response) => {
                info!("Access token found, requesting new Copilot token...");

                // Create HTTP client
                let client = reqwest::Client::new();

                // Get new Copilot token
                match auth::get_copilot_token(
                    &client,
                    &config.github.copilot_token_url,
                    &access_token_response.access_token,
                )
                .await
                {
                    Ok(copilot_token) => {
                        // Save the new token (to custom path if specified)
                        let copilot_token_path = self.copilot_token_path.as_deref().map(Path::new);
                        storage::save_token_to_path(&copilot_token, copilot_token_path)?;
                        info!("✓ Copilot token refreshed successfully!");
                        info!("Token expires at: {}", copilot_token.expires_at);
                        Ok(())
                    }
                    Err(e) => {
                        info!("✗ Failed to refresh Copilot token: {}", e);
                        info!("You may need to run --login to re-authenticate");
                        Err(e)
                    }
                }
            }
            None => {
                info!("✗ No access token found on disk");
                info!("Please run with --login first to authenticate with GitHub");
                Err(anyhow::anyhow!("No access token found"))
            }
        }
    }

    /// Verify that required token exists before starting server
    pub fn verify_token_exists(&self) -> Result<()> {
        // Check if we have a valid token (from custom or default path)
        let token_exists = if let Some(ref path) = self.copilot_token_path {
            let p = Path::new(path);
            if !p.exists() {
                info!("✗ Specified Copilot token file does not exist: {}", path);
                info!("Please run with --login to authenticate with GitHub");
                return Err(anyhow::anyhow!("Copilot token file not found: {}", path));
            }
            true
        } else {
            storage::token_exists()
        };

        if !token_exists {
            info!("No authentication token found.");
            info!("Please run with --login to authenticate with GitHub");
            return Err(anyhow::anyhow!(
                "No authentication token found. Run with --login to authenticate."
            ));
        }

        Ok(())
    }
}
