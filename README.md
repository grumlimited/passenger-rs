# passenger-rs

[![CI](https://github.com/grumlimited/passenger-rs/workflows/CI/badge.svg)](https://github.com/grumlimited/passenger-rs/actions)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

A Rust proxy server that exposes GitHub Copilot models through a streaming OpenAI-compatible API.

**Only streaming responses are supported.** All requests must include `"stream": true`.

## Use Case

Point any OpenAI-compatible client at `http://127.0.0.1:8081` to use GitHub Copilot models transparently:

```rust
use rig::providers::openai;

let client = openai::Client::builder()
    .api_key("no-key")
    .base_url("http://127.0.0.1:8081/v1")
    .build()?;

let model = client.completion_model("claude-sonnet-4.5");
```

Or use any other OpenAI-compatible SDK, tool, or application by pointing it at the proxy.

## Quick Start

### 1. Download or build

```bash
# From source
git clone https://github.com/grumlimited/passenger-rs.git
cd passenger-rs
cargo build --release
```

Or download a pre-built binary from the [releases page](https://github.com/grumlimited/passenger-rs/releases).

### 2. Authenticate with GitHub

```bash
./passenger-rs --login
```

This will:

1. Display a GitHub device code and URL
2. Open your browser to https://github.com/login/device
3. After authorization, save tokens to `~/.config/passenger-rs/`

### 3. Start the proxy server

```bash
./passenger-rs
```

The server starts on `http://127.0.0.1:8081` by default.

### 4. Test the connection

```bash
curl http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": true
  }'
```

## API Endpoints

### POST /v1/chat/completions

OpenAI-compatible streaming chat completions. Requires `"stream": true`.

Routes to Copilot `/responses` for gpt-5+ models (excluding gpt-5-mini), and to Copilot `/chat/completions` for all other models.

**Example request:**

```json
{
  "model": "gpt-4o",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "stream": true
}
```

Response is a `text/event-stream` of `ChatCompletionChunk` SSE events.

### POST /v1/responses

OpenAI Responses API streaming endpoint. Requires `"stream": true`.

The Copilot Responses SSE format is identical to the OpenAI Responses SSE format — the stream is passed through byte-for-byte.

### GET /v1/models

Lists available models from the GitHub Copilot model catalog in OpenAI format.

### GET /health

Returns `200 OK` with body `OK`.

## Installation

### System Requirements

- Rust 1.70 or later
- Active GitHub Copilot subscription

### System Service (Linux)

Pre-built packages for Ubuntu and Arch Linux are available on the [releases page](https://github.com/grumlimited/passenger-rs/releases).

```bash
# Arch Linux
yay -U passenger-rs-0.0.1-1-x86_64.pkg.tar.zst

# Ubuntu/Debian
sudo dpkg -i passenger-rs-0.0.1-x86_64.deb
```

Manage with systemd:

```bash
systemctl --user start passenger-rs.service
systemctl --user enable passenger-rs.service
systemctl --user status passenger-rs.service
```

**Note:** Before starting the service, authenticate with `--login`.

## Configuration

Edit `config.toml`:

```toml
[github]
device_code_url = "https://github.com/login/device/code"
oauth_token_url = "https://github.com/login/oauth/access_token"
copilot_token_url = "https://api.github.com/copilot_internal/v2/token"
copilot_models_url = "https://models.github.ai/catalog/models"
client_id = "Iv1.b507a08c87ecfe98"

[copilot]
api_base_url = "https://api.githubcopilot.com"

[server]
port = 8081
host = "127.0.0.1"
```

## CLI Reference

```
passenger-rs - GitHub Copilot streaming proxy (OpenAI-compatible)

Usage: passenger-rs [OPTIONS]

Options:
  -c, --config <CONFIG>
          Path to the configuration file [default: config.toml]

      --login
          Perform GitHub OAuth device flow login

      --refresh-token
          Refresh Copilot token using existing access token

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

## Token Management

Tokens are stored in `~/.config/passenger-rs/`:

- **Access Token** (`access_token.json`): Long-lived, used to obtain Copilot tokens
- **Copilot Token** (`token.json`): Short-lived (~25 minutes), auto-refreshed 60 seconds before expiry

Custom paths:

```bash
./passenger-rs --login \
  --access-token-path /custom/path/access_token.json \
  --copilot-token-path /custom/path/copilot_token.json
```

## Development

```bash
cargo build          # debug build
cargo build --release
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

## Troubleshooting

**"No authentication token found"**
```bash
./passenger-rs --login
```

**"Only streaming is supported"**

Add `"stream": true` to your request body.

**"Address already in use"**

Change `port` in `config.toml`, or kill the process using port 8081:
```bash
lsof -ti:8081 | xargs kill -9
```

**Enable debug logging**
```bash
RUST_LOG=debug ./passenger-rs
```

## License

GPL-3.0 — see [LICENSE](LICENSE).

## Acknowledgments

- Based on [copilot-to-api](https://github.com/Alorse/copilot-to-api)
- Built with [Axum](https://github.com/tokio-rs/axum), [Tokio](https://tokio.rs/), [Clap](https://github.com/clap-rs/clap)

## Disclaimer

Ensure you comply with GitHub's Terms of Service and Copilot's usage policies.
