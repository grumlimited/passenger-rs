pub mod models;
pub mod responses;

/// Returns `true` when the given model ID should be proxied to Copilot's
/// `/responses` endpoint rather than `/chat/completions`.
///
/// Mirrors the TypeScript predicate in `provider/provider.ts`:
/// ```ts
/// function shouldUseCopilotResponsesApi(modelID: string): boolean {
///   const match = /^gpt-(\d+)/.exec(modelID)
///   if (!match) return false
///   return Number(match[1]) >= 5 && !modelID.startsWith("gpt-5-mini")
/// }
/// ```
///
/// In practice today: only `gpt-5`, `gpt-5-nano`, `gpt-5-*` (excluding
/// `gpt-5-mini`) map to `/responses`. Everything else — `o1-*`, `o3-*`,
/// `o4-*`, `claude-*`, `gemini-*`, `gpt-4*`, `gpt-5-mini` — uses
/// `/chat/completions`.
pub fn should_use_responses_api(model_id: &str) -> bool {
    // Must start with "gpt-" followed by one or more digits.
    let after_gpt = match model_id.strip_prefix("gpt-") {
        Some(s) => s,
        None => return false,
    };

    // Parse the leading integer from the rest of the model string.
    let digits: String = after_gpt
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return false;
    }
    let version: u64 = match digits.parse() {
        Ok(n) => n,
        Err(_) => return false,
    };

    version >= 5 && !model_id.starts_with("gpt-5-mini")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Models that SHOULD use /responses
    #[test]
    fn test_gpt5_uses_responses() {
        assert!(should_use_responses_api("gpt-5"));
    }

    #[test]
    fn test_gpt5_nano_uses_responses() {
        assert!(should_use_responses_api("gpt-5-nano"));
    }

    #[test]
    fn test_gpt5_turbo_uses_responses() {
        assert!(should_use_responses_api("gpt-5-turbo"));
    }

    #[test]
    fn test_gpt6_uses_responses() {
        // Future-proofing: any gpt-N where N >= 5 (excl. gpt-5-mini)
        assert!(should_use_responses_api("gpt-6"));
    }

    // Models that should NOT use /responses (use /chat/completions instead)
    #[test]
    fn test_gpt5_mini_uses_chat_completions() {
        assert!(!should_use_responses_api("gpt-5-mini"));
    }

    #[test]
    fn test_gpt4o_uses_chat_completions() {
        assert!(!should_use_responses_api("gpt-4o"));
    }

    #[test]
    fn test_gpt4_turbo_uses_chat_completions() {
        assert!(!should_use_responses_api("gpt-4-turbo"));
    }

    #[test]
    fn test_o1_uses_chat_completions() {
        assert!(!should_use_responses_api("o1"));
    }

    #[test]
    fn test_o3_mini_uses_chat_completions() {
        assert!(!should_use_responses_api("o3-mini"));
    }

    #[test]
    fn test_o4_mini_uses_chat_completions() {
        assert!(!should_use_responses_api("o4-mini"));
    }

    #[test]
    fn test_claude_uses_chat_completions() {
        assert!(!should_use_responses_api("claude-3-7-sonnet"));
    }

    #[test]
    fn test_gemini_uses_chat_completions() {
        assert!(!should_use_responses_api("gemini-2.0-flash"));
    }

    #[test]
    fn test_llama_uses_chat_completions() {
        assert!(!should_use_responses_api("llama3.2"));
    }

    #[test]
    fn test_empty_string_uses_chat_completions() {
        assert!(!should_use_responses_api(""));
    }

    #[test]
    fn test_gpt_no_version_uses_chat_completions() {
        // "gpt-" with no digits after
        assert!(!should_use_responses_api("gpt-preview"));
    }
}
