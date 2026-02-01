# passenger-rs

[![CI](https://github.com/grumlimited/passenger-rs/workflows/CI/badge.svg)](https://github.com/grumlimited/passenger-rs/actions)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

A high-performance Rust-based proxy server that converts GitHub Copilot into an OpenAI-compatible API.

## 🚀 Features

- **GitHub OAuth Authentication**: Secure device flow authentication with GitHub
- **Token Management**: Automatic token caching, validation, and refresh
- **OpenAI Compatibility**: Drop-in replacement for OpenAI API clients
- **Custom Token Paths**: Flexible token storage locations
- **Health Monitoring**: Built-in health check endpoint
- **Request/Response Transformation**: Seamless conversion between OpenAI and Copilot formats
- **High Performance**: Built with Rust, Axum, and Tokio for maximum efficiency

## 📋 Table of Contents

- [Quick Start](#-quick-start)
- [Installation](#-installation)
- [Usage](#-usage)
- [Configuration](#-configuration)
- [Architecture](#-architecture)
- [API Endpoints](#-api-endpoints)
- [CLI Reference](#-cli-reference)
- [Development](#-development)
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

### Using with OpenAI SDKs

#### Python

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8081/v1",
    api_key="dummy"  # API key not required
)

response = client.chat.completions.create(
    model="gpt-4",
    messages=[
        {"role": "user", "content": "Write a Python function to calculate fibonacci numbers"}
    ]
)

print(response.choices[0].message.content)
```

#### Node.js

```javascript
import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: 'http://127.0.0.1:8081/v1',
  apiKey: 'dummy' // API key not required
});

const response = await client.chat.completions.create({
  model: 'gpt-4',
  messages: [{ role: 'user', content: 'Explain async/await in JavaScript' }]
});

console.log(response.choices[0].message.content);
```

#### cURL

```bash
# Chat completion
curl http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {"role": "system", "content": "You are a helpful coding assistant."},
      {"role": "user", "content": "How do I reverse a string in Rust?"}
    ],
    "temperature": 0.7,
    "max_tokens": 500
  }'

# List available models
curl http://127.0.0.1:8081/v1/models

# Health check
curl http://127.0.0.1:8081/health
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

### Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          passenger-rs                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │  main.rs │  │ clap.rs  │  │  config.rs   │  │  login.rs   │ │
│  │          │  │          │  │              │  │             │ │
│  │ App      │─►│ CLI      │  │ Config       │  │ OAuth Flow  │ │
│  │ Entry    │  │ Parser   │  │ Loader       │  │ Handler     │ │
│  └──────────┘  └──────────┘  └──────────────┘  └─────────────┘ │
│                      │                                            │
│                      ▼                                            │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                     server.rs                               │ │
│  │  ┌──────────────┐  ┌───────────────┐  ┌─────────────────┐ │ │
│  │  │ Axum Router  │  │ Chat          │  │ List Models     │ │ │
│  │  │              │─►│ Completions   │  │ Endpoint        │ │ │
│  │  │ HTTP Server  │  │ Endpoint      │  │                 │ │ │
│  │  └──────────────┘  └───────────────┘  └─────────────────┘ │ │
│  └────────────────────────────────────────────────────────────┘ │
│                      │                                            │
│                      ▼                                            │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────┐            │
│  │  auth.rs    │  │ storage.rs   │  │ token_      │            │
│  │             │  │              │  │ manager.rs  │            │
│  │ OAuth +     │  │ Token        │  │             │            │
│  │ Copilot     │─►│ Persistence  │◄─│ Validation  │            │
│  │ Token API   │  │              │  │ & Refresh   │            │
│  └─────────────┘  └──────────────┘  └─────────────┘            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Module Descriptions

#### Core Modules

- **`main.rs`** (59 lines)
  - Application entry point
  - Initializes logging and configuration
  - Delegates to CLI handlers or starts server

- **`clap.rs`** (157 lines)
  - CLI argument parsing using Clap
  - Command handlers (`--login`, `--refresh-token`)
  - Token validation before server startup

- **`server.rs`**
  - Axum web server initialization
  - Route definitions and middleware
  - Shared application state management

#### Authentication & Token Management

- **`auth.rs`**
  - GitHub OAuth device flow implementation
  - Copilot token request/exchange
  - HTTP client with proper headers (Firefox UA)

- **`login.rs`**
  - Interactive login flow with progress spinner
  - Device code polling with exponential backoff
  - Success/failure user feedback

- **`storage.rs`**
  - Token persistence to filesystem
  - Support for custom token paths
  - JSON serialization/deserialization
  - Parent directory validation

- **`token_manager.rs`**
  - Token expiration checking
  - Automatic token refresh logic
  - Cache management

#### API Handlers

- **`server_chat_completion.rs`**
  - OpenAI to Copilot request transformation
  - Copilot to OpenAI response transformation
  - Handles optional `created` field (defaults to current timestamp)

- **`server_list_models.rs`**
  - Fetches models from GitHub Copilot catalog
  - Transforms to OpenAI models format
  - Error handling and fallback

#### Configuration

- **`config.rs`**
  - TOML configuration file parsing
  - Structured config types
  - Default values and validation

### Request Flow

```
1. Client Request (OpenAI Format)
   │
   ▼
2. Axum Router (/v1/chat/completions)
   │
   ▼
3. Token Manager (Load/Refresh Token)
   │
   ▼
4. Request Transform (OpenAI → Copilot)
   │
   ▼
5. GitHub Copilot API Call
   │
   ▼
6. Response Transform (Copilot → OpenAI)
   │
   ▼
7. Client Response (OpenAI Format)
```

### Token Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Token Lifecycle                                              │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  1. Login Command                                            │
│     └─► GitHub OAuth Device Flow                            │
│         └─► Get Device Code                                 │
│         └─► User Authorizes on GitHub                       │
│         └─► Poll for Access Token                           │
│         └─► Exchange for Copilot Token                      │
│         └─► Save to ~/.config/passenger-rs/                 │
│                                                               │
│  2. Server Request                                           │
│     └─► Load Token from Cache                               │
│     └─► Check Expiration (60s buffer)                       │
│     └─► If Expired:                                         │
│         └─► Load Access Token                               │
│         └─► Request New Copilot Token                       │
│         └─► Save to Cache                                   │
│     └─► Use Token for API Call                             │
│                                                               │
│  3. Refresh Command                                          │
│     └─► Load Access Token                                   │
│     └─► Request New Copilot Token                           │
│     └─► Save to Cache                                       │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## 🔌 API Endpoints

### POST /v1/chat/completions

OpenAI-compatible chat completions endpoint.

**Request:**
```json
{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
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

**Supported Parameters:**
- `model` - Model identifier (forwarded to Copilot)
- `messages` - Array of message objects
- `temperature` - Sampling temperature (0-2)
- `max_tokens` - Maximum tokens to generate
- `top_p` - Nucleus sampling parameter
- `n` - Number of completions
- `stop` - Stop sequences

**Note:** Streaming is not yet supported. The `stream` parameter is ignored.

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

### GET /health

Health check endpoint for monitoring.

**Response:** `"OK"` (HTTP 200)

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

### Command Examples

```bash
# Standard login
./passenger-rs --login

# Login with custom token locations
./passenger-rs --login \
  --access-token-path /secure/vault/access.json \
  --copilot-token-path /secure/vault/copilot.json

# Refresh token manually
./passenger-rs --refresh-token

# Run server with custom config
./passenger-rs --config production.toml

# Run server with custom token path
./passenger-rs --copilot-token-path /secure/vault/copilot.json
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

### Project Structure

```
passenger-rs/
├── src/
│   ├── main.rs                    # Application entry point (59 lines)
│   ├── clap.rs                    # CLI command handlers (157 lines)
│   ├── lib.rs                     # Library exports
│   ├── auth.rs                    # GitHub OAuth + Copilot token API
│   ├── config.rs                  # Configuration management
│   ├── login.rs                   # Interactive login flow
│   ├── storage.rs                 # Token persistence layer
│   ├── token_manager.rs           # Token validation & refresh
│   ├── server.rs                  # Axum server setup
│   ├── server_chat_completion.rs  # Chat completions endpoint
│   └── server_list_models.rs      # Models listing endpoint
├── tests/
│   ├── auth_tests.rs              # Authentication integration tests
│   └── chat_completions_test.rs   # API endpoint tests
├── config.toml                    # Runtime configuration
├── Cargo.toml                     # Dependencies and metadata
└── README.md                      # This file
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

### Test Coverage

The project includes:
- **35 unit tests** covering core functionality
- **2 integration tests** for API endpoints
- **Mock-based tests** for external APIs using wiremock
- **Real API tests** (marked with `#[ignore]`) for manual verification

### Test Categories

**Unit Tests:**
- Config loading and validation
- Token expiration checking
- Storage operations
- Request/response parsing
- OAuth flow components

**Integration Tests:**
- Full authentication flow
- Chat completions endpoint
- Error handling
- Custom token paths

### Continuous Integration

```bash
# Run full CI checks
cargo fmt --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test
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

### Benchmarks

_(Benchmarks to be added in future versions)_

## 🗺️ Roadmap

- [ ] Streaming support for chat completions
- [ ] Embeddings endpoint
- [ ] Rate limiting
- [ ] Metrics and observability (Prometheus/OpenTelemetry)
- [ ] Docker image
- [ ] Multi-user support
- [ ] Load balancing across multiple tokens
- [ ] WebSocket support
- [ ] Request caching

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Run linter (`cargo clippy`)
6. Format code (`cargo fmt`)
7. Commit your changes (`git commit -m 'Add amazing feature'`)
8. Push to the branch (`git push origin feature/amazing-feature`)
9. Open a Pull Request

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
