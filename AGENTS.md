# AGENTS.md — passenger-rs

Streaming-only OpenAI-compatible proxy for GitHub Copilot. Reads `config.toml`
from the working directory (or a path supplied via `--config`). Requires a
Copilot token obtained by running `cargo run -- --login` once before starting
the server.

---

## Commands

```sh
cargo fmt --all                          # format (CI enforces this)
cargo clippy --all-targets --all-features -- -D warnings   # lint (CI -D warnings)
cargo test                               # all unit tests (fast, no network)
cargo test <test_name>                   # run a single test by name
cargo run -- --login                     # interactive GitHub OAuth flow, stores token
cargo run                                # start proxy on 127.0.0.1:8081
cargo run -- --config /path/to/config.toml  # custom config path
```

MSRV is **1.93.1** (pinned in CI matrix).

---

## Test quirks

- Most tests in `tests/chat_completions_test.rs` are `#[ignore]` — they need
  a real Copilot token and hit the live API.
- `cargo test` only runs the non-ignored unit tests; those pass without
  credentials.
- To run a live integration test: `cargo test test_chat_completions_with_real_api -- --ignored`
- `config::tests::test_config_from_file` reads `config.toml` from CWD; must
  be run from the repo root.

---

## Architecture

```
src/
  copilot/          raw Copilot API types and routing logic
    mod.rs          should_use_responses_api() — routing predicate
    models.rs       CopilotModelsResponse / CopilotModel (matches /models JSON)
    responses/      Copilot /responses request, response, SSE stream types
  openai/           OpenAI-compatible types returned to clients
    chat_completions/  request, response, conversion from Copilot
    responses/         thin re-export of copilot types (wire format is identical)
    models.rs          OpenAIModelsResponse converted from CopilotModelsResponse
  server/
    chat_completions.rs  main handler — routes to /responses or /chat/completions
    models.rs            GET /v1/models handler
    responses.rs         POST /v1/responses handler (pass-through SSE)
    mod.rs               router, AppState, AppError
  config.rs         loads config.toml via toml crate
  token_manager.rs  caches/refreshes short-lived Copilot tokens
```

---

## Routing logic (non-obvious)

`should_use_responses_api()` in `src/copilot/mod.rs`:
- Routes `gpt-5`, `gpt-5-nano`, `gpt-5-turbo`, `gpt-6+` → Copilot `/responses` endpoint
- Routes everything else (`claude-*`, `gemini-*`, `gpt-4*`, `o1/o3/o4-*`, `gpt-5-mini`) → `/chat/completions`
- The `/responses` path needs SSE translation (Copilot events → ChatCompletionChunk)
- The `/chat/completions` path is a byte-for-byte pass-through (already OpenAI SSE)

---

## Key constraints

- **Streaming only.** All endpoints reject requests without `"stream": true`
  with `400 Bad Request`. Never add non-streaming response paths.
- **Tool call indices.** Copilot's `output_index` counts all output items;
  OpenAI's `tool_calls[].index` counts only tool calls. The translation layer
  maintains its own `tool_index_map` — don't conflate the two.
- **Model visibility filter.** `GET /v1/models` only returns models where
  `model_picker_enabled: true` in the upstream payload. The filter lives in
  `CopilotModelsResponse::visible_models()`.
- **`CopilotModel` schema** matches the `{ "object": "list", "data": [...] }`
  shape returned by `https://api.githubcopilot.com/models`. The old
  `models.dev` shape (nested under `github-copilot.models`) is gone.
- **`created` field** in `OpenAIModel` is `u64` (not `u32`) to match the
  OpenAI spec unix timestamp range.
- **Header for Copilot requests:** all upstream requests use
  `Copilot-Integration-Id: vscode-chat`.

---

## CI gate

CI runs: `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test`

A PR to `master` must pass all three.
