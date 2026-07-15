//! Converters between OpenAI `/responses` types and Copilot `/responses` types.
//!
//! # OpenAI → Copilot request
//! `OpenAIResponsesRequest` → `CopilotResponsesRequest`: field copy, Copilot-only
//! extension fields (`prompt_cache_key`, `safety_identifier`) default to `None`.

use crate::copilot::responses::request::CopilotResponsesRequest;
use crate::openai::responses::request::OpenAIResponsesRequest;

// ---------------------------------------------------------------------------
// OpenAI request → Copilot request
// ---------------------------------------------------------------------------

impl From<OpenAIResponsesRequest> for CopilotResponsesRequest {
    fn from(req: OpenAIResponsesRequest) -> Self {
        CopilotResponsesRequest {
            model: req.model,
            input: req.input,
            stream: req.stream,
            temperature: req.temperature,
            top_p: req.top_p,
            max_output_tokens: req.max_output_tokens,
            tools: req.tools,
            tool_choice: req.tool_choice,
            instructions: req.instructions,
            store: req.store,
            previous_response_id: req.previous_response_id,
            reasoning: req.reasoning,
            truncation: req.truncation,
            text: req.text,
            metadata: req.metadata,
            user: req.user,
            service_tier: req.service_tier,
            parallel_tool_calls: req.parallel_tool_calls,
            max_tool_calls: req.max_tool_calls,
            // Copilot-only fields — not sent by OpenAI clients
            prompt_cache_key: None,
            safety_identifier: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::copilot::responses::request::CopilotResponsesRequest;
    use crate::openai::responses::request::OpenAIResponsesRequest;

    fn minimal_openai_request() -> OpenAIResponsesRequest {
        OpenAIResponsesRequest {
            model: "gpt-4o".to_string(),
            input: vec![],
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            tools: None,
            tool_choice: None,
            instructions: None,
            store: None,
            previous_response_id: None,
            reasoning: None,
            truncation: None,
            text: None,
            metadata: None,
            user: None,
            service_tier: None,
            parallel_tool_calls: None,
            max_tool_calls: None,
        }
    }

    // --- OpenAI → Copilot request ---

    #[test]
    fn test_openai_to_copilot_request_basic_fields() {
        let openai_req = minimal_openai_request();
        let copilot_req: CopilotResponsesRequest = openai_req.into();
        assert_eq!(copilot_req.model, "gpt-4o");
        assert!(copilot_req.prompt_cache_key.is_none());
        assert!(copilot_req.safety_identifier.is_none());
    }

    #[test]
    fn test_openai_to_copilot_request_preserves_stream_flag() {
        let mut req = minimal_openai_request();
        req.stream = Some(true);
        req.temperature = Some(0.8);
        req.max_output_tokens = Some(512);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.stream, Some(true));
        assert_eq!(copilot.temperature, Some(0.8));
        assert_eq!(copilot.max_output_tokens, Some(512));
    }

    #[test]
    fn test_openai_to_copilot_request_instructions_preserved() {
        let mut req = minimal_openai_request();
        req.instructions = Some("Be concise.".to_string());
        req.user = Some("user-42".to_string());
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.instructions.as_deref(), Some("Be concise."));
        assert_eq!(copilot.user.as_deref(), Some("user-42"));
    }

    #[test]
    fn test_openai_to_copilot_no_extension_fields() {
        // Verify the Copilot-only fields are always None when converting from OpenAI
        let copilot: CopilotResponsesRequest = minimal_openai_request().into();
        assert!(copilot.prompt_cache_key.is_none());
        assert!(copilot.safety_identifier.is_none());
    }
}
