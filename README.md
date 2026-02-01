# passenger-rs

A Rust-based proxy server that converts GitHub Copilot into an OpenAI-compatible API.

## Features

- GitHub OAuth device flow authentication
- Token caching and automatic refresh
- OpenAI-compatible `/v1/chat/completions` endpoint
- OpenAI-compatible `/v1/models` endpoint
- Health check endpoint
- Request/response transformation between OpenAI and Copilot formats

## Quick Start

### 1. Build the project

```bash
cargo build --release
```

### 2. Authenticate with GitHub

```bash
cargo run --release -- --login
```

This will:
1. Display a GitHub device code
2. Prompt you to visit https://github.com/login/device
3. After authorization, save your Copilot token to `~/.config/passenger-rs/token.json`

### 3. Start the proxy server

```bash
cargo run --release
```

The server will start on `http://127.0.0.1:8080` by default.

## Usage

### Chat Completions

Send OpenAI-compatible requests to the proxy:

```bash
curl http://127.0.0.1:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [
      {"role": "user", "content": "Hello, how are you?"}
    ]
  }'
```

### List Models

```bash
curl http://127.0.0.1:8080/v1/models
```

### Health Check

```bash
curl http://127.0.0.1:8080/health
```

## Configuration

Edit `config.toml` to customize:

```toml
[github]
device_code_url = "https://github.com/login/device/code"
oauth_token_url = "https://github.com/login/oauth/access_token"
copilot_token_url = "https://api.github.com/copilot_internal/v2/token"
client_id = "Iv1.b507a08c87ecfe98"

[copilot]
api_base_url = "https://api.githubcopilot.com"

[server]
port = 8080
host = "127.0.0.1"
```

## Use with OpenAI SDK

You can use this proxy with any OpenAI-compatible client:

### Python

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8080/v1",
    api_key="dummy"  # API key not required
)

response = client.chat.completions.create(
    model="gpt-4",
    messages=[
        {"role": "user", "content": "Hello!"}
    ]
)

print(response.choices[0].message.content)
```

### Node.js

```javascript
import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: 'http://127.0.0.1:8080/v1',
  apiKey: 'dummy' // API key not required
});

const response = await client.chat.completions.create({
  model: 'gpt-4',
  messages: [{ role: 'user', content: 'Hello!' }]
});

console.log(response.choices[0].message.content);
```

## Command-Line Options

```
Usage: passenger-rs [OPTIONS]

Options:
  -c, --config <PATH>  Path to the configuration file [default: config.toml]
      --login          Perform GitHub OAuth device flow login
  -h, --help          Print help
  -V, --version       Print version
```

## Token Management

- Tokens are stored in `~/.config/passenger-rs/token.json`
- Tokens are automatically refreshed when expired
- Tokens expire after approximately 25 minutes
- If refresh fails, run `--login` again to re-authenticate

## Architecture

```
┌─────────────┐         ┌──────────────┐         ┌──────────────────┐
│   OpenAI    │ OpenAI  │  passenger-rs │ Copilot │ GitHub Copilot   │
│   Client    ├────────►│  Proxy Server├────────►│  API             │
│             │ Format  │              │ Format  │                  │
└─────────────┘         └──────────────┘         └──────────────────┘
                              │
                              │ Token Management
                              ▼
                        ┌──────────────┐
                        │  Token Cache │
                        │  ~/.config/  │
                        └──────────────┘
```

## Project Structure

```
passenger-rs/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library exports
│   ├── auth.rs           # GitHub OAuth + Copilot token logic
│   ├── config.rs         # Configuration loading
│   ├── login.rs          # Interactive login flow
│   ├── storage.rs        # Token persistence
│   ├── token_manager.rs  # Token validation and refresh
│   └── server.rs         # Axum web server + API handlers
├── config.toml           # Runtime configuration
├── Cargo.toml            # Dependencies
└── README.md            # This file
```

## Testing

```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib

# Run with output
cargo test -- --nocapture
```

## Development

```bash
# Check code without building
cargo check

# Run clippy linter
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt
```

## Requirements

- Rust 1.70 or later
- Active GitHub Copilot subscription
- Internet connection for GitHub OAuth and Copilot API

## License

MIT

## Acknowledgments

Based on the [copilot-to-api](https://github.com/Alorse/copilot-to-api) project.
