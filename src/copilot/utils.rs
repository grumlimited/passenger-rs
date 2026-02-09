use crate::copilot::{CopilotChatRequest, CopilotChatResponse, CopilotMessage};
use crate::openai::completion::models::OpenAIChatRequest;
use crate::openai::responses::models::prompt_request::Content::InputText;
use crate::openai::responses::models::prompt_request::PromptRequest;
use crate::openai::responses::models::prompt_response::{
    CompletionResponse, Output, ResponsesUsage,
};
use crate::server_chat_completion::{CopilotChoice, CopilotUsage};

impl From<OpenAIChatRequest> for CopilotChatRequest {
    fn from(request: OpenAIChatRequest) -> Self {
        Self {
            messages: request
                .messages
                .iter()
                .map(|m| CopilotMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    padding: None,
                    tool_calls: m.tool_calls.clone(),
                    tool_call_id: m.tool_call_id.clone(),
                    name: m.name.clone(),
                })
                .collect(),
            model: request.model.clone(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(request.stream),
            tools: request.tools,
            tool_choice: request.tool_choice,
        }
    }
}

impl From<PromptRequest> for CopilotChatRequest {
    fn from(value: PromptRequest) -> Self {
        use crate::openai::completion::models::{FunctionDefinition, Tool as OpenAITool};

        // Convert messages from PromptRequest format to CopilotMessage format
        let mut messages: Vec<CopilotMessage> = value
            .input
            .iter()
            .map(|m| {
                // Extract text content from Content enum
                let content = m
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        InputText { text } => Some(text.clone()),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                CopilotMessage {
                    role: m.role.clone(),
                    content: Some(content),
                    padding: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                }
            })
            .collect();

        // Add system message with instructions at the beginning
        messages.insert(
            0,
            CopilotMessage {
                role: "system".to_string(),
                content: Some(value.instructions),
                padding: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        );

        // Convert tools from PromptRequest format to OpenAI Tool format
        let tools = if value.tools.is_empty() {
            None
        } else {
            Some(
                value
                    .tools
                    .iter()
                    .map(|t| {
                        // Convert ToolParameters to JSON Value for FunctionDefinition
                        let parameters = serde_json::json!({
                            "type": t.parameters.param_type,
                            "properties": t.parameters.properties,
                            "required": t.parameters.required,
                            "additionalProperties": t.parameters.additional_properties,
                        });

                        OpenAITool {
                            tool_type: t.tool_type.clone(),
                            function: FunctionDefinition {
                                name: t.name.clone(),
                                description: Some(t.description.clone()),
                                parameters,
                            },
                        }
                    })
                    .collect(),
            )
        };

        Self {
            messages,
            model: value.model,
            temperature: None,
            max_tokens: Some(value.max_output_tokens),
            stream: Some(false),
            tools,
            tool_choice: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::responses::models::prompt_request::PromptRequest;
    use serde_json;

    #[test]
    fn test_prompt_request_to_copilot_chat_request() {
        // Load rig_openai_prompt_request.json as string
        let json = include_str!("../resources/rig_openai_prompt_request.json");

        // Parse PromptRequest
        let prompt_request: PromptRequest =
            serde_json::from_str(json).expect("Failed to parse PromptRequest");

        // Convert to CopilotChatRequest
        let copilot_request: CopilotChatRequest = prompt_request.into();

        // Check model field
        assert_eq!(copilot_request.model, "claude-sonnet-4.5");

        // Check system instructions message
        assert_eq!(copilot_request.messages[0].role, "system");
        assert!(copilot_request.messages[0]
            .content
            .as_ref()
            .unwrap()
            .contains("Return a comma-separated list of ticker symbols"));

        // Check user message
        assert_eq!(copilot_request.messages[1].role, "user");
        assert!(copilot_request.messages[1]
            .content
            .as_ref()
            .unwrap()
            .starts_with("Extract the ticker symbols"));

        // Check max_tokens
        assert_eq!(copilot_request.max_tokens, Some(2000));

        // Check tools conversion
        assert!(copilot_request.tools.is_some());
        assert_eq!(copilot_request.tools.as_ref().unwrap().len(), 2);
        assert_eq!(
            copilot_request.tools.as_ref().unwrap()[0].function.name,
            "get_portfolio_tickers"
        );
        assert_eq!(
            copilot_request.tools.as_ref().unwrap()[1].function.name,
            "get_portfolio"
        );
    }
}


impl From<CompletionResponse> for CopilotChatResponse {
    fn from(resp: CompletionResponse) -> Self {
        // Map usage
        let usage = resp.usage.map(|u| CopilotUsage::from(u));
        // Map choices
        let choices = resp
            .output
            .into_iter()
            .enumerate()
            .map(|(i, output)| match output {
                Output::Message(msg) => CopilotChoice {
                    index: Some(i as u32),
                    message: CopilotMessage {
                        role: msg.role.to_string(),
                        content: msg.content.get(0).and_then(|c| match c {
                            crate::openai::responses::models::prompt_response::AssistantContent::OutputText(text) => Some(text.text.clone()),
                            crate::openai::responses::models::prompt_response::AssistantContent::Refusal { refusal } => Some(refusal.clone()),
                        }),
                        padding: None,
                        tool_calls: None, // TODO if tool calls appear in OutputMessage, support mapping
                        tool_call_id: None,
                        name: None,
                    },
                    finish_reason: "stop".to_string(),
                },
                Output::FunctionCall(fc) => CopilotChoice {
                    index: Some(i as u32),
                    message: CopilotMessage {
                        role: "assistant".to_string(),
                        content: None,
                        padding: None,
                        tool_calls: Some(vec![crate::openai::completion::models::ToolCall {
                            id: Some(fc.id),
                            tool_type: "function".to_string(),
                            function: crate::openai::completion::models::FunctionCall {
                                name: fc.name.clone(),
                                arguments: fc.arguments.to_string(),
                            },
                        }]),
                        tool_call_id: Some(fc.call_id),
                        name: Some(fc.name),
                    },
                    finish_reason: "function_call".to_string(),
                },
                Output::Reasoning { id: _, summary } => CopilotChoice {
                    index: Some(i as u32),
                    message: CopilotMessage {
                        role: "assistant".to_string(),
                        content: Some(summary.iter().map(|s| match s {
                            crate::openai::responses::models::prompt_response::ReasoningSummary::SummaryText { text } => text.clone(),
                        }).collect::<Vec<_>>().join("\n")),
                        padding: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    },
                    finish_reason: "reasoning".to_string(),
                },
            })
            .collect();
        Self {
            id: resp.id,
            created: Some(resp.created_at),
            model: resp.model,
            choices,
            usage,
        }
    }
}

impl From<ResponsesUsage> for CopilotUsage {
    fn from(u: ResponsesUsage) -> Self {
        CopilotUsage {
            prompt_tokens: u.input_tokens as u32,
            completion_tokens: u.output_tokens as u32,
            total_tokens: u.total_tokens as u32,
        }
    }
}

