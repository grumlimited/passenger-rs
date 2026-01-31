# passenger-rs — simple proxy for Copilot OpenAPI endpoints

What I built
- A minimal Rust proxy server (Axum + Reqwest) that forwards requests under `/proxy/*` to a configurable upstream (default: GitHub API).
- Replaces/upgrades the Authorization header with the server-held `COPILOT_API_KEY` (if configured).
- Simple per-IP rate limiting (requests/minute) and hop-by-hop header filtering.

How to run
1. Copy `.env.example` to `.env` and set `COPILOT_BASE_URL` and `COPILOT_API_KEY` as needed.
2. `cargo run --release`
3. Send requests to `http://127.0.0.1:3000/proxy/<path>` — the proxy will forward to `<COPILOT_BASE_URL>/<path>` preserving query and most headers.

Notes and next steps
- TLS termination, authentication for clients, improved rate limiting (redis/leaky-bucket), metrics, retries/backoff, streaming responses, and request/response logging are natural next steps.
- Be careful with licensing and terms of service when proxying third-party APIs.
