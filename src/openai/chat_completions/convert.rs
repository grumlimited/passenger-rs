//! Converters between OpenAI `/chat/completions` types and Copilot `/responses` types.
//!
//! # Chat request → Copilot responses request
//! Maps `messages` → Copilot `input` items, adapts tools/tool_choice schemas,
//! and carries over all standard parameters.
//!
//! In the Copilot Responses API multi-turn format, assistant tool calls are
//! separate `FunctionCallItem` input items (not nested inside the assistant
//! message). This matches how opencode sends them.
//!
//! # Copilot responses response → Chat response
//! Maps `output` items back into `choices[0]`, collecting text from
//! `MessageOutputItem` and tool calls from `FunctionCallOutputItem`.

use crate::copilot::responses::request::{
    AssistantContentPart, AssistantMessage as CopilotAssistantMessage, CopilotResponsesRequest,
    FunctionCallItem, FunctionCallItemKind, FunctionTool, InputImage, InputItem,
    SystemMessage as CopilotSystemMessage, Tool, ToolChoice, ToolChoiceFunction, ToolChoiceMode,
    ToolResult, ToolResultKind, UserContent as CopilotUserContent,
    UserContentPart as CopilotUserContentPart, UserMessage as CopilotUserMessage,
};
use crate::copilot::responses::response::{
    CopilotResponsesResponse, OutputItem, Usage as CopilotUsage,
};
use crate::openai::chat_completions::request::{
    ChatCompletionsRequest, ChatMessage, ChatTool, ChatToolChoice, ChatToolChoiceMode,
    SystemContent, UserContent, UserContentPart,
};
use crate::openai::chat_completions::request::{ToolCall, ToolCallFunction, ToolCallKind};
use crate::openai::chat_completions::response::{
    AssistantResponseMessage, ChatChoice, ChatCompletionsResponse, ChatUsage,
    CompletionTokensDetails, PromptTokensDetails,
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

            // Tool calls become separate FunctionCallItem input items
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
        ChatTool::Function(f) => Tool::Function(FunctionTool {
            name: f.name,
            description: f.description,
            parameters: f.parameters,
            strict: f.strict,
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
            reasoning: None,
            truncation: None,
            text: None,
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

// ---------------------------------------------------------------------------
// CopilotResponsesResponse → ChatCompletionsResponse
// ---------------------------------------------------------------------------

impl From<CopilotResponsesResponse> for ChatCompletionsResponse {
    fn from(resp: CopilotResponsesResponse) -> Self {
        let mut text_parts: Vec<String> = vec![];
        let mut tool_calls: Vec<ToolCall> = vec![];
        let mut finish_reason: Option<String> = Some("stop".to_string());

        for item in resp.output {
            match item {
                OutputItem::Message(msg) => {
                    for part in msg.content {
                        text_parts.push(part.text);
                    }
                }
                OutputItem::FunctionCall(fc) => {
                    tool_calls.push(ToolCall {
                        id: fc.call_id,
                        kind: ToolCallKind::Function,
                        function: ToolCallFunction {
                            name: fc.name,
                            arguments: fc.arguments,
                        },
                    });
                    finish_reason = Some("tool_calls".to_string());
                }
                _ => {} // reasoning, web_search, etc. — not surfaced in chat completions
            }
        }

        let content = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join(""))
        };

        let choice = ChatChoice {
            index: 0,
            message: AssistantResponseMessage {
                role: Some("assistant".to_string()),
                content,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                reasoning_text: None,
                reasoning_opaque: None,
            },
            finish_reason,
        };

        let usage = map_usage(resp.usage);

        ChatCompletionsResponse {
            id: Some(resp.id),
            created: Some(resp.created_at),
            model: Some(resp.model),
            choices: vec![choice],
            usage: Some(usage),
        }
    }
}

fn map_usage(u: CopilotUsage) -> ChatUsage {
    ChatUsage {
        prompt_tokens: Some(u.input_tokens),
        completion_tokens: Some(u.output_tokens),
        total_tokens: Some(u.input_tokens + u.output_tokens),
        prompt_tokens_details: u.input_tokens_details.map(|d| PromptTokensDetails {
            cached_tokens: d.cached_tokens,
        }),
        completion_tokens_details: u.output_tokens_details.map(|d| CompletionTokensDetails {
            reasoning_tokens: d.reasoning_tokens,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot::responses::response::{
        FunctionCallOutputItem, MessageOutputItem, OutputItem, OutputTextKind, OutputTextPart,
        Usage,
    };
    use crate::openai::chat_completions::request::{
        AssistantMessage, ChatMessage, SystemMessage, ToolMessage, UserMessage,
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

    fn copilot_text_response(text: &str) -> CopilotResponsesResponse {
        CopilotResponsesResponse {
            id: "resp-1".to_string(),
            created_at: 1700000000,
            model: "gpt-4o".to_string(),
            output: vec![OutputItem::Message(MessageOutputItem {
                id: "msg-1".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputTextPart {
                    kind: OutputTextKind::OutputText,
                    text: text.to_string(),
                    logprobs: None,
                    annotations: vec![],
                }],
            })],
            usage: Usage {
                input_tokens: 10,
                input_tokens_details: None,
                output_tokens: 5,
                output_tokens_details: None,
            },
            error: None,
            service_tier: None,
            incomplete_details: None,
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
        use crate::openai::chat_completions::request::{ChatFunctionTool, ChatTool};
        let mut req = minimal_chat_request(vec![]);
        req.tools = Some(vec![ChatTool::Function(ChatFunctionTool {
            name: "get_weather".to_string(),
            description: Some("Get weather".to_string()),
            parameters: serde_json::json!({"type": "object"}),
            strict: None,
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
        // The tool call becomes a FunctionCallItem input item
        assert_eq!(copilot.input.len(), 1);
        match &copilot.input[0] {
            InputItem::FunctionCall(fc) => {
                assert_eq!(fc.name, "get_weather");
                assert_eq!(fc.call_id, "call-1");
            }
            other => panic!("expected FunctionCall, got {:?}", other),
        }
    }

    // --- CopilotResponsesResponse → ChatCompletionsResponse ---

    #[test]
    fn test_text_response_maps_to_choice() {
        let resp = copilot_text_response("Hello, world!");
        let chat: ChatCompletionsResponse = resp.into();
        assert_eq!(
            chat.choices[0].message.content.as_deref(),
            Some("Hello, world!")
        );
        assert_eq!(chat.choices[0].finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_function_call_response_maps_to_tool_calls() {
        let resp = CopilotResponsesResponse {
            id: "resp-2".to_string(),
            created_at: 1700000000,
            model: "gpt-4o".to_string(),
            output: vec![OutputItem::FunctionCall(FunctionCallOutputItem {
                id: "item-1".to_string(),
                call_id: "call-abc".to_string(),
                name: "get_weather".to_string(),
                arguments: "{\"city\":\"Paris\"}".to_string(),
            })],
            usage: Usage {
                input_tokens: 5,
                input_tokens_details: None,
                output_tokens: 20,
                output_tokens_details: None,
            },
            error: None,
            service_tier: None,
            incomplete_details: None,
        };
        let chat: ChatCompletionsResponse = resp.into();
        let tc = &chat.choices[0].message.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.function.name, "get_weather");
        assert_eq!(tc.id, "call-abc");
        assert_eq!(chat.choices[0].finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn test_usage_maps_correctly() {
        let resp = copilot_text_response("Hi");
        let chat: ChatCompletionsResponse = resp.into();
        let usage = chat.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(10));
        assert_eq!(usage.completion_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(15));
    }

    #[test]
    fn test_response_id_and_model_preserved() {
        let resp = copilot_text_response("Hi");
        let chat: ChatCompletionsResponse = resp.into();
        assert_eq!(chat.id.as_deref(), Some("resp-1"));
        assert_eq!(chat.model.as_deref(), Some("gpt-4o"));
        assert_eq!(chat.created, Some(1700000000));
    }
}
