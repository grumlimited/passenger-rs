//! Converters between OpenAI `/chat/completions` types and Copilot `/responses` types.
//!
//! # Chat request → Copilot responses request
//! Maps `messages` → Copilot `input` items, adapts tools/tool_choice schemas,
//! and carries over all standard parameters.
//!
//! Notable mappings:
//! - `max_tokens` → `max_output_tokens`
//! - `response_format` → `text.format`
//! - `reasoning_effort` → `reasoning.effort`
//! - `verbosity` → `text.verbosity`
//! - `thinking_budget` is silently dropped (no Copilot equivalent)
//! - `frequency_penalty`, `presence_penalty`, `stop`, `seed` have no Copilot equivalent
//!
//! In the Copilot Responses API multi-turn format, assistant tool calls are
//! separate `FunctionCallItem` input items (not nested inside the assistant
//! message). In the Copilot API, `FunctionCallItem.id` is the item ID
//! (typically the same as `call_id` for OpenAI-origin calls) and
//! `FunctionCallItem.call_id` is the function call correlation ID that must
//! match the `ToolResult.call_id` in the next turn.

use crate::copilot::responses::request::{
    AssistantContentPart, AssistantMessage as CopilotAssistantMessage, CopilotResponsesRequest,
    FunctionCallItem, FunctionCallItemKind, FunctionTool, InputImage, InputItem, ReasoningConfig,
    SystemMessage as CopilotSystemMessage, TextConfig, TextFormat, Tool, ToolChoice,
    ToolChoiceFunction, ToolChoiceMode, ToolResult, ToolResultKind,
    UserContent as CopilotUserContent, UserContentPart as CopilotUserContentPart,
    UserMessage as CopilotUserMessage,
};
use crate::openai::chat_completions::request::{
    ChatCompletionsRequest, ChatMessage, ChatTool, ChatToolChoice, ChatToolChoiceMode,
    ResponseFormat, SystemContent, UserContent, UserContentPart,
};

// ---------------------------------------------------------------------------
// Chat messages → Copilot input items
// ---------------------------------------------------------------------------

fn convert_chat_message(msg: ChatMessage) -> Vec<InputItem> {
    match msg {
        ChatMessage::System(s) => {
            let content = match s.content {
                SystemContent::Text(t) => t,
                SystemContent::Parts(parts) => parts
                    .into_iter()
                    .map(|p| p.text)
                    .collect::<Vec<_>>()
                    .join(""),
            };
            vec![InputItem::SystemMessage(CopilotSystemMessage { content })]
        }

        ChatMessage::User(u) => {
            let content = match u.content {
                UserContent::Text(t) => CopilotUserContent::Text(t),
                UserContent::Parts(parts) => {
                    let converted: Vec<CopilotUserContentPart> = parts
                        .into_iter()
                        .map(|p| match p {
                            UserContentPart::Text { text } => {
                                CopilotUserContentPart::InputText { text }
                            }
                            UserContentPart::ImageUrl { image_url } => {
                                CopilotUserContentPart::InputImage(InputImage {
                                    image_url: Some(image_url.url),
                                    detail: None,
                                    file_id: None,
                                })
                            }
                        })
                        .collect();
                    CopilotUserContent::Parts(converted)
                }
            };
            vec![InputItem::UserMessage(CopilotUserMessage { content })]
        }

        ChatMessage::Assistant(a) => {
            let mut items: Vec<InputItem> = vec![];

            // Text content as AssistantMessage
            let text = a.content.unwrap_or_default();
            if !text.is_empty() {
                items.push(InputItem::AssistantMessage(CopilotAssistantMessage {
                    content: vec![AssistantContentPart::OutputText { text }],
                    id: None,
                }));
            }

            // Tool calls become separate FunctionCallItem input items.
            // In the Copilot Responses API:
            //   - `id` is the item ID (we use the OpenAI tool_call id)
            //   - `call_id` is the correlation ID that must match ToolResult.call_id
            // For OpenAI-origin tool calls these are the same value.
            for tc in a.tool_calls.unwrap_or_default() {
                items.push(InputItem::FunctionCall(FunctionCallItem {
                    kind: FunctionCallItemKind::FunctionCall,
                    id: tc.id.clone(),
                    call_id: tc.id,
                    name: tc.function.name,
                    arguments: tc.function.arguments,
                }));
            }

            // If nothing was added (e.g. assistant msg with no content or tool calls), add empty
            if items.is_empty() {
                items.push(InputItem::AssistantMessage(CopilotAssistantMessage {
                    content: vec![],
                    id: None,
                }));
            }

            items
        }

        ChatMessage::Tool(t) => {
            vec![InputItem::ToolResult(ToolResult {
                kind: ToolResultKind::FunctionCallOutput,
                call_id: t.tool_call_id,
                output: t.content,
            })]
        }
    }
}

// ---------------------------------------------------------------------------
// Chat tools → Copilot tools
// ---------------------------------------------------------------------------

fn convert_chat_tool(tool: ChatTool) -> Tool {
    match tool {
        ChatTool::Function(wrapper) => Tool::Function(FunctionTool {
            name: wrapper.function.name,
            description: wrapper.function.description,
            parameters: wrapper.function.parameters,
            strict: wrapper.function.strict,
        }),
    }
}

fn convert_chat_tool_choice(choice: ChatToolChoice) -> ToolChoice {
    match choice {
        ChatToolChoice::Mode(mode) => ToolChoice::Mode(match mode {
            ChatToolChoiceMode::None => ToolChoiceMode::None,
            ChatToolChoiceMode::Auto => ToolChoiceMode::Auto,
            ChatToolChoiceMode::Required => ToolChoiceMode::Required,
        }),
        ChatToolChoice::Function(f) => ToolChoice::Function(ToolChoiceFunction {
            kind: f.kind,
            name: f.function.name,
        }),
    }
}

// ---------------------------------------------------------------------------
// response_format → text.format
// ---------------------------------------------------------------------------

fn convert_response_format(fmt: ResponseFormat) -> TextFormat {
    match fmt {
        ResponseFormat::Text => {
            // No direct `text` variant in Copilot's TextFormat — treated as plain text (no format)
            // Callers should leave `text.format` as None for `ResponseFormat::Text`.
            // This arm is unreachable via convert_text_config, but kept for completeness.
            TextFormat::JsonObject // fallback: shouldn't be reached
        }
        ResponseFormat::JsonObject => TextFormat::JsonObject,
        ResponseFormat::JsonSchema { json_schema } => TextFormat::JsonSchema {
            name: json_schema.name,
            description: json_schema.description,
            schema: json_schema.schema,
            strict: None,
        },
    }
}

fn convert_text_config(
    response_format: Option<ResponseFormat>,
    verbosity: Option<String>,
) -> Option<TextConfig> {
    let format = match response_format {
        // `Text` means "no special format" — don't set text.format at all
        None | Some(ResponseFormat::Text) => None,
        Some(fmt) => Some(convert_response_format(fmt)),
    };

    if format.is_none() && verbosity.is_none() {
        None
    } else {
        Some(TextConfig { format, verbosity })
    }
}

// ---------------------------------------------------------------------------
// ChatCompletionsRequest → CopilotResponsesRequest
// ---------------------------------------------------------------------------

impl From<ChatCompletionsRequest> for CopilotResponsesRequest {
    fn from(req: ChatCompletionsRequest) -> Self {
        let input: Vec<InputItem> = req
            .messages
            .into_iter()
            .flat_map(convert_chat_message)
            .collect();

        let tools = req
            .tools
            .map(|ts| ts.into_iter().map(convert_chat_tool).collect());

        let tool_choice = req.tool_choice.map(convert_chat_tool_choice);

        // reasoning.effort comes from reasoning_effort; thinking_budget has no equivalent
        let reasoning = req.reasoning_effort.map(|effort| ReasoningConfig {
            effort: Some(effort),
            summary: None,
        });

        // text.format comes from response_format; text.verbosity from verbosity
        let text = convert_text_config(req.response_format, req.verbosity);

        CopilotResponsesRequest {
            model: req.model,
            input,
            stream: req.stream,
            temperature: req.temperature,
            top_p: req.top_p,
            max_output_tokens: req.max_tokens,
            tools,
            tool_choice,
            instructions: None,
            store: None,
            previous_response_id: None,
            reasoning,
            truncation: None,
            text,
            metadata: None,
            user: req.user,
            service_tier: None,
            parallel_tool_calls: None,
            max_tool_calls: None,
            prompt_cache_key: None,
            safety_identifier: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot::responses::request::{InputItem, TextFormat, ToolChoice, ToolChoiceMode};
    use crate::openai::chat_completions::request::{
        AssistantMessage, ChatMessage, JsonSchemaConfig, ResponseFormat, SystemMessage, ToolMessage,
        UserMessage,
    };

    fn minimal_chat_request(msgs: Vec<ChatMessage>) -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "gpt-4o".to_string(),
            messages: msgs,
            stream: None,
            stream_options: None,
            temperature: None,
            top_p: None,
            max_tokens: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            seed: None,
            response_format: None,
            tools: None,
            tool_choice: None,
            user: None,
            reasoning_effort: None,
            verbosity: None,
            thinking_budget: None,
        }
    }

    // --- ChatCompletionsRequest → CopilotResponsesRequest ---

    #[test]
    fn test_system_message_converts() {
        let req = minimal_chat_request(vec![ChatMessage::System(SystemMessage {
            content: crate::openai::chat_completions::request::SystemContent::Text(
                "Be helpful.".to_string(),
            ),
        })]);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.input.len(), 1);
        match &copilot.input[0] {
            InputItem::SystemMessage(s) => assert_eq!(s.content, "Be helpful."),
            other => panic!("expected SystemMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_user_message_text_converts() {
        let req = minimal_chat_request(vec![ChatMessage::User(UserMessage {
            content: UserContent::Text("Hello".to_string()),
        })]);
        let copilot: CopilotResponsesRequest = req.into();
        match &copilot.input[0] {
            InputItem::UserMessage(u) => match &u.content {
                CopilotUserContent::Text(t) => assert_eq!(t, "Hello"),
                _ => panic!("expected text"),
            },
            other => panic!("expected UserMessage, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_message_converts() {
        let req = minimal_chat_request(vec![ChatMessage::Tool(ToolMessage {
            content: "{\"result\":42}".to_string(),
            tool_call_id: "call-1".to_string(),
        })]);
        let copilot: CopilotResponsesRequest = req.into();
        match &copilot.input[0] {
            InputItem::ToolResult(tr) => {
                assert_eq!(tr.call_id, "call-1");
                assert!(tr.output.contains("42"));
            }
            other => panic!("expected ToolResult, got {:?}", other),
        }
    }

    #[test]
    fn test_max_tokens_maps_to_max_output_tokens() {
        let mut req = minimal_chat_request(vec![]);
        req.max_tokens = Some(512);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.max_output_tokens, Some(512));
    }

    #[test]
    fn test_tool_choice_auto_maps() {
        let mut req = minimal_chat_request(vec![]);
        req.tool_choice = Some(ChatToolChoice::Mode(ChatToolChoiceMode::Auto));
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(
            copilot.tool_choice,
            Some(ToolChoice::Mode(ToolChoiceMode::Auto))
        );
    }

    #[test]
    fn test_function_tool_maps() {
        use crate::openai::chat_completions::request::{
            ChatFunctionTool, ChatFunctionToolWrapper, ChatTool,
        };
        let mut req = minimal_chat_request(vec![]);
        req.tools = Some(vec![ChatTool::Function(ChatFunctionToolWrapper {
            function: ChatFunctionTool {
                name: "get_weather".to_string(),
                description: Some("Get weather".to_string()),
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            },
        })]);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.tools.as_ref().unwrap().len(), 1);
        match &copilot.tools.as_ref().unwrap()[0] {
            Tool::Function(f) => {
                assert_eq!(f.name, "get_weather");
                assert_eq!(f.description.as_deref(), Some("Get weather"));
            }
            other => panic!("expected Function tool, got {:?}", other),
        }
    }

    #[test]
    fn test_assistant_with_tool_calls_expands_to_separate_items() {
        use crate::openai::chat_completions::request::{ToolCall, ToolCallFunction, ToolCallKind};
        let req = minimal_chat_request(vec![ChatMessage::Assistant(AssistantMessage {
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call-1".to_string(),
                kind: ToolCallKind::Function,
                function: ToolCallFunction {
                    name: "get_weather".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            reasoning_text: None,
            reasoning_opaque: None,
        })]);
        let copilot: CopilotResponsesRequest = req.into();
        assert_eq!(copilot.input.len(), 1);
        match &copilot.input[0] {
            InputItem::FunctionCall(fc) => {
                assert_eq!(fc.name, "get_weather");
                // call_id (correlation ID) must match the OpenAI tool_call id
                assert_eq!(fc.call_id, "call-1");
                // item id also set to the same value for OpenAI-origin calls
                assert_eq!(fc.id, "call-1");
            }
            other => panic!("expected FunctionCall, got {:?}", other),
        }
    }

    #[test]
    fn test_response_format_json_object_maps_to_text_format() {
        let mut req = minimal_chat_request(vec![]);
        req.response_format = Some(ResponseFormat::JsonObject);
        let copilot: CopilotResponsesRequest = req.into();
        let text = copilot.text.expect("expected text config");
        assert!(matches!(text.format, Some(TextFormat::JsonObject)));
    }

    #[test]
    fn test_response_format_json_schema_maps() {
        let mut req = minimal_chat_request(vec![]);
        req.response_format = Some(ResponseFormat::JsonSchema {
            json_schema: JsonSchemaConfig {
                name: "my_schema".to_string(),
                description: Some("desc".to_string()),
                schema: serde_json::json!({"type": "object"}),
            },
        });
        let copilot: CopilotResponsesRequest = req.into();
        let text = copilot.text.expect("expected text config");
        match text.format {
            Some(TextFormat::JsonSchema {
                name,
                description,
                schema: _,
                strict: _,
            }) => {
                assert_eq!(name, "my_schema");
                assert_eq!(description.as_deref(), Some("desc"));
            }
            other => panic!("expected JsonSchema format, got {:?}", other),
        }
    }

    #[test]
    fn test_response_format_text_produces_no_text_config() {
        let mut req = minimal_chat_request(vec![]);
        req.response_format = Some(ResponseFormat::Text);
        let copilot: CopilotResponsesRequest = req.into();
        // ResponseFormat::Text means "no special format" — text config should be None
        assert!(copilot.text.is_none());
    }

    #[test]
    fn test_reasoning_effort_maps_to_reasoning_config() {
        let mut req = minimal_chat_request(vec![]);
        req.reasoning_effort = Some("high".to_string());
        let copilot: CopilotResponsesRequest = req.into();
        let reasoning = copilot.reasoning.expect("expected reasoning config");
        assert_eq!(reasoning.effort.as_deref(), Some("high"));
    }

    #[test]
    fn test_verbosity_maps_to_text_config() {
        let mut req = minimal_chat_request(vec![]);
        req.verbosity = Some("detailed".to_string());
        let copilot: CopilotResponsesRequest = req.into();
        let text = copilot.text.expect("expected text config");
        assert_eq!(text.verbosity.as_deref(), Some("detailed"));
        assert!(text.format.is_none());
    }

    #[test]
    fn test_both_response_format_and_verbosity_coexist() {
        let mut req = minimal_chat_request(vec![]);
        req.response_format = Some(ResponseFormat::JsonObject);
        req.verbosity = Some("brief".to_string());
        let copilot: CopilotResponsesRequest = req.into();
        let text = copilot.text.expect("expected text config");
        assert!(matches!(text.format, Some(TextFormat::JsonObject)));
        assert_eq!(text.verbosity.as_deref(), Some("brief"));
    }
}
