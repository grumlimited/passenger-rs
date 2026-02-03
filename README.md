# passenger-rs

[![CI](https://github.com/grumlimited/passenger-rs/workflows/CI/badge.svg)](https://github.com/grumlimited/passenger-rs/actions)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

A high-performance Rust-based proxy server that converts GitHub Copilot into OpenAI-compatible and Ollama-compatible APIs.

## 💡 Use Case: Rig Integration

This project enables using GitHub Copilot models with [Rig](https://github.com/0xPlaygrounds/rig) and other Ollama-compatible frameworks:

```rust
use rig::providers::ollama;

let client = ollama::Client::builder()
    .base_url("http://127.0.0.1:8081/v1")
    .build()?;

let model = client.completion_model("claude-sonnet-4.5");

let agent = AgentBuilder::new(model)
    .preamble("You're an AI assistant powered by GitHub Copilot")
    .name("copilot-agent")
    .max_tokens(2000)
    .build();
```

## 🚀 Features

- **GitHub OAuth Authentication**: Secure device flow authentication with GitHub
- **Token Management**: Automatic token caching, validation, and refresh
- **OpenAI Compatibility**: Drop-in replacement for OpenAI API clients
- **Ollama Compatibility**: Ollama-format responses via `/v1/api/chat` endpoint
- **Custom Token Paths**: Flexible token storage locations
- **Health Monitoring**: Built-in health check endpoint
- **Request/Response Transformation**: Seamless conversion between OpenAI, Ollama, and Copilot formats

## 📋 Table of Contents

- [Quick Start](#-quick-start)
- [Installation](#-installation)
- [Running as a System Service](#-running-as-a-system-service)
- [Usage](#-usage)
- [Configuration](#-configuration)
- [Architecture](#-architecture)
- [API Endpoints](#-api-endpoints)
- [CLI Reference](#️-cli-reference)
- [Development](#️-development)
- [Testing](#-testing)
- [Troubleshooting](#-troubleshooting)

## 🏁 Quick Start

### 1. Build the project

```bash
cargo build --release
```

### 2. Authenticate with GitHub

```bash
./target/release/passenger-rs -- --login
```

This will:

1. Display a GitHub device code and URL
2. Open your browser to https://github.com/login/device
3. After authorization, save tokens to `~/.config/passenger-rs/`

### 3. Start the proxy server

```bash
./target/release/passenger-rs
```

The server will start on `http://127.0.0.1:8081` by default.

### 4. Test the connection

**OpenAI format:**

```bash
curl http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {"role": "user", "content": "Hello, how are you?"}
    ]
  }'
```

**Ollama format:**

```bash
curl http://127.0.0.1:8081/v1/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {"role": "user", "content": "Hello, how are you?"}
    ]
  }'
```

## 📦 Installation

### From Source

```bash
git clone https://github.com/yourusername/passenger-rs.git
cd passenger-rs
cargo build --release
```

The binary will be available at `target/release/passenger-rs`.

### System Requirements

- Rust 1.70 or later
- Active GitHub Copilot subscription
- Internet connection for GitHub OAuth and Copilot API

## 🔧 Running as a System Service

Pre-built packages for Ubuntu and Arch Linux are available on the [releases page](https://github.com/grumlimited/passenger-rs/releases).

### Arch Linux Installation

Install using your AUR helper:

```bash
yay -U passenger-rs-0.0.1-1-x86_64.pkg.tar.zst
```

### Ubuntu/Debian Installation

```bash
sudo dpkg -i passenger-rs-0.0.1-x86_64.deb
```

### Managing the Service

The package includes a systemd user service that can be managed with standard systemctl commands:

```bash
# Start the service
systemctl --user start passenger-rs.service

# Enable auto-start on login
systemctl --user enable passenger-rs.service

# Check service status
systemctl --user status passenger-rs.service
```

**Example output:**

```
● passenger-rs.service - passenger-rs - GitHub Copilot Proxy
     Loaded: loaded (/usr/lib/systemd/user/passenger-rs.service; disabled; preset: enabled)
     Active: active (running) since Tue 2026-02-03 22:44:17 CET; 1s ago
     [...]
     INFO passenger_rs: OpenAI API endpoint: http://127.0.0.1:8081/v1/chat/completions
     INFO passenger_rs: Ollama API endpoint: http://127.0.0.1:8081/v1/api/chat
     INFO passenger_rs: Models endpoint: http://127.0.0.1:8081/v1/models
```

**Note:** Before starting the service, you must authenticate with GitHub Copilot using `--login` (see [Usage](#-usage)).

## 🎯 Usage

### Basic Usage

```bash
# Start the server with default configuration
./passenger-rs

# Use custom configuration file
./passenger-rs --config /path/to/config.toml

# Authenticate with GitHub
./passenger-rs --login

# Refresh expired token
./passenger-rs --refresh-token
```

### Custom Token Paths

You can specify custom locations for token storage:

```bash
# Login with custom token paths
./passenger-rs --login \
  --access-token-path /custom/path/access_token.json \
  --copilot-token-path /custom/path/copilot_token.json

# Refresh token using custom paths
./passenger-rs --refresh-token \
  --access-token-path /custom/path/access_token.json \
  --copilot-token-path /custom/path/copilot_token.json

# Start server with custom copilot token path
./passenger-rs --copilot-token-path /custom/path/copilot_token.json
```

## ⚙️ Configuration

Edit `config.toml` to customize the proxy behavior:

```toml
[github]
# GitHub OAuth device code endpoint
device_code_url = "https://github.com/login/device/code"

# GitHub OAuth access token endpoint
oauth_token_url = "https://github.com/login/oauth/access_token"

# GitHub Copilot token endpoint
copilot_token_url = "https://api.github.com/copilot_internal/v2/token"

# GitHub Copilot models catalog
copilot_models_url = "https://models.github.ai/catalog/models"

# GitHub Copilot public client ID (same for all users)
client_id = "Iv1.b507a08c87ecfe98"

[copilot]
# GitHub Copilot API base URL
api_base_url = "https://api.githubcopilot.com"

[server]
# Port to listen on
port = 8081

# Host to bind to
host = "127.0.0.1"
```

### Environment Variables

Currently, configuration is file-based. Environment variable support may be added in future versions.

## 🏗️ Architecture

### High-Level Overview

```
┌─────────────────┐         ┌──────────────────┐         ┌──────────────────┐
│   OpenAI Client │ OpenAI  │   passenger-rs   │ Copilot │ GitHub Copilot   │
│   (Any SDK)     ├────────►│   Proxy Server   ├────────►│  API             │
│                 │ Format  │                  │ Format  │                  │
└─────────────────┘         └──────────────────┘         └──────────────────┘
                                     │
                                     │ OAuth Flow
                                     ▼
                            ┌─────────────────┐
                            │  GitHub OAuth   │
                            │  Device Flow    │
                            └─────────────────┘
                                     │
                                     │ Token Storage
                                     ▼
                            ┌─────────────────┐
                            │  Token Cache    │
                            │  ~/.config/     │
                            │  passenger-rs/  │
                            └─────────────────┘
```

## 🔌 API Endpoints

### POST /v1/chat/completions

OpenAI-compatible chat completions endpoint.

**Request:**

```json
{
  "model": "gpt-4",
  "messages": [
    {
      "role": "system",
      "content": "You are a helpful assistant."
    },
    {
      "role": "user",
      "content": "Hello!"
    }
  ],
  "temperature": 0.7,
  "max_tokens": 100
}
```

**Response:**

```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "gpt-4",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you today?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 10,
    "total_tokens": 22
  }
}
```
**Note:** Streaming is not yet supported. The `stream` parameter is ignored.

### POST /v1/api/chat

Ollama-compatible chat endpoint.

**Request:**

```json
{
  "model": "gpt-4",
  "messages": [
    {
      "role": "user",
      "content": "Hello!"
    }
  ],
  "temperature": 0.7,
  "max_tokens": 100
}
```

**Response:**

```json
{
  "model": "gpt-4",
  "created_at": "2023-11-07T05:31:56Z",
  "message": {
    "role": "assistant",
    "content": "Hello! How can I help you today?"
  },
  "done": true,
  "done_reason": "stop",
  "prompt_eval_count": 12,
  "eval_count": 10
}
```

**Note:** This endpoint accepts OpenAI-format requests but returns Ollama-format responses for compatibility with Ollama clients.

### GET /v1/models

Lists available models from GitHub Copilot catalog.

**Response:**

```json
{
  "object": "list",
  "data": [
    {
      "id": "gpt-4",
      "object": "model",
      "created": 1677652288,
      "owned_by": "openai"
    }
  ]
}
```

## 🖥️ CLI Reference

```
passenger-rs - GitHub Copilot to OpenAI API Proxy

Usage: passenger-rs [OPTIONS]

Options:
  -c, --config <CONFIG>
          Path to the configuration file
          [default: config.toml]

      --login
          Perform GitHub OAuth device flow login
          Initiates interactive authentication with GitHub

      --refresh-token
          Refresh Copilot token using existing access token
          Useful when Copilot token expires

      --access-token-path <ACCESS_TOKEN_PATH>
          Path to the access token file
          [default: ~/.config/passenger-rs/access_token.json]

      --copilot-token-path <COPILOT_TOKEN_PATH>
          Path to the Copilot token file
          [default: ~/.config/passenger-rs/token.json]

  -h, --help
          Print help information

  -V, --version
          Print version information
```

## 🛠️ Development

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version
cargo --version
```

### Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Check without building (fast)
cargo check
```

### Code Quality

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run clippy linter
cargo clippy --all-targets --all-features -- -D warnings

# Fix clippy warnings automatically
cargo clippy --fix
```

## 🧪 Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_chat_completions_without_auth

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test '*'

# Run ignored tests (require real authentication)
cargo test -- --ignored
```

## 🐛 Troubleshooting

### Common Issues

#### "No authentication token found"

**Solution:**

```bash
./passenger-rs --login
```

#### "Access token file does not exist"

You specified a custom access token path but the file doesn't exist.

**Solution:**

```bash
# Login will create the token at the default location
./passenger-rs --login

# Then copy to your custom location, or re-login with custom path
./passenger-rs --login --access-token-path /custom/path/access.json
```

#### "Failed to refresh Copilot token: 401 Unauthorized"

Your access token has expired or is invalid.

**Solution:**

```bash
./passenger-rs --login
```

#### "Address already in use"

Another process is using port 8081.

**Solutions:**

```bash
# Option 1: Change port in config.toml
[server]
port = 8082

# Option 2: Find and kill the process
lsof -ti:8081 | xargs kill -9
```

#### "Connection refused" when making API calls

Server is not running.

**Solution:**

```bash
./passenger-rs
```

### Debug Mode

Enable debug logging:

```bash
RUST_LOG=debug ./passenger-rs
```

### Token Inspection

```bash
# View token details
cat ~/.config/passenger-rs/token.json | jq

# Check expiration
jq '.expires_at' ~/.config/passenger-rs/token.json
```

## 📝 Token Management

### Token Locations

By default, tokens are stored in:

- **Access Token**: `~/.config/passenger-rs/access_token.json`
- **Copilot Token**: `~/.config/passenger-rs/token.json`

### Token Lifecycle

- **Access Token**: Long-lived, used to obtain Copilot tokens
- **Copilot Token**: Short-lived (~25 minutes), auto-refreshed
- **Expiration Buffer**: Tokens refresh 60 seconds before expiration

### Manual Token Refresh

```bash
# Refresh using default paths
./passenger-rs --refresh-token

# Refresh using custom paths
./passenger-rs --refresh-token \
  --access-token-path /path/to/access.json \
  --copilot-token-path /path/to/copilot.json
```

### Security Considerations

- Tokens contain sensitive credentials
- Store tokens in secure locations with appropriate permissions
- Consider using encrypted filesystems for token storage
- Never commit tokens to version control

```bash
# Set secure permissions
chmod 600 ~/.config/passenger-rs/*.json
```

## 🚀 Performance

- **Language**: Rust for memory safety and performance
- **Async Runtime**: Tokio for efficient concurrency
- **Web Framework**: Axum for fast HTTP handling
- **HTTP Client**: Reqwest with connection pooling

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

### GPL-3.0 Summary

This means you can:

- ✅ Use the software for any purpose
- ✅ Study and modify the source code
- ✅ Share the software with others
- ✅ Share your modifications

**Important**: If you distribute modified versions, you must:

- 📝 Make the source code available
- 🔓 License it under GPL-3.0
- 📋 Document your changes
- 📄 Include the original copyright notice

## 🙏 Acknowledgments

- Based on the [copilot-to-api](https://github.com/Alorse/copilot-to-api) project
- Built with [Axum](https://github.com/tokio-rs/axum) web framework
- Uses [Tokio](https://tokio.rs/) async runtime
- CLI powered by [Clap](https://github.com/clap-rs/clap)

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/yourusername/passenger-rs/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/passenger-rs/discussions)

## ⚠️ Disclaimer

This project is for educational purposes. Make sure you comply with GitHub's Terms of Service and Copilot's usage policies.
