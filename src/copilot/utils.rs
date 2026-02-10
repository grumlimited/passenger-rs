use crate::copilot::{CopilotChatRequest, CopilotChatResponse, CopilotMessage};
use crate::openai::completion::models::{
    FunctionCall, OpenAIChatRequest, ToolCall as CompletionToolCall,
};
use crate::openai::responses::models::prompt_request::Content::InputText;
use crate::openai::responses::models::prompt_request::PromptRequest;
use crate::openai::responses::models::prompt_response::{
    AdditionalParameters, AssistantContent, OutputFunctionCall, OutputMessage, OutputRole,
    OutputTokensDetails, ResponseObject, ResponseStatus, ResponsesToolDefinition, Text, ToolStatus,
};
use crate::openai::responses::models::prompt_response::{
    CompletionResponse, Output, ResponsesUsage,
};
use crate::server_chat_completion::CopilotUsage;

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

        let mut messages: Vec<CopilotMessage> = vec![];

        // Add a system message with instructions at the beginning
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

        let mut users = value
            .input
            .iter()
            .filter(|message| matches!(&message.role, role if role == &Some("user".to_string())))
            .map(|message| {
                let content = match &message.content {
                    Some(contents) => contents
                        .iter()
                        .map(|e| match e {
                            InputText { text } => text.clone(),
                        })
                        .collect::<Vec<String>>()
                        .join("\n"),
                    _ => "".to_string(),
                };

                CopilotMessage {
                    role: "user".to_string(),
                    content: Some(content),
                    padding: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                }
            })
            .collect::<Vec<CopilotMessage>>();

        messages.append(&mut users);

        /*
         * If empty, then it will be a condition to also not add an "assistant" section to the built up copilot request
         */
        let function_call_messages_tool_calls: Vec<CompletionToolCall> = value
            .input
            .iter()
            .filter(|message| matches!(&message.message_type, message_type if message_type == "function_call"))
            .enumerate()
            .map(|(id, message)| CompletionToolCall {
                id: Some(format!("{}", id)),
                tool_type: "function".to_string(),
                function: FunctionCall {
                    name: message
                        .name
                        .clone()
                        .unwrap_or("SHOULD HAVE BEEN SET".to_string()),
                    arguments: message
                        .arguments
                        .clone()
                        .unwrap_or("SHOULD HAVE BEEN SET".to_string()),
                },
            })
            .collect();

        if !function_call_messages_tool_calls.is_empty() {
            let function_call_message = CopilotMessage {
                role: "assistant".to_string(),
                content: None,
                padding: None,
                tool_calls: Some(function_call_messages_tool_calls),
                tool_call_id: None,
                name: None,
            };

            let tool_calls = match function_call_message.tool_calls {
                Some(ref tool_calls) => tool_calls.as_slice(),
                _ => &[],
            };

            let mut function_call_output_messages: Vec<CopilotMessage> = value
                .input
                .iter()
                .filter(|message| matches!(&message.message_type, message_type if message_type == "function_call_output"))
                .zip(tool_calls)
                .enumerate()
                .map(|(id, (message, tool_call))| CopilotMessage {
                    role: "tool".to_string(),
                    content: message.output.clone(),
                    padding: None,
                    tool_calls: None,
                    tool_call_id: Some(format!("{}", id)),
                    name: Some(tool_call.function.name.clone()),
                })
                .collect();

            messages.insert(messages.len(), function_call_message);
            messages.append(&mut function_call_output_messages);
        }

        // Convert tools from PromptRequest format to OpenAI Tool format
        let tools = if value.tools.is_empty() {
            None
        } else {
            Some(
                value
                    .tools
                    .iter()
                    .map(|tool| {
                        // Convert ToolParameters to JSON Value for FunctionDefinition
                        let parameters = serde_json::json!({
                            "type": tool.parameters.param_type,
                            "properties": tool.parameters.properties,
                            "required": tool.parameters.required,
                            "additionalProperties": tool.parameters.additional_properties,
                        });

                        OpenAITool {
                            tool_type: tool.tool_type.clone(),
                            function: FunctionDefinition {
                                name: tool.name.clone(),
                                description: Some(tool.description.clone()),
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

impl From<CopilotChatResponse> for CompletionResponse {
    fn from(resp: CopilotChatResponse) -> Self {
        // usage mapping
        let usage = resp.usage.map(ResponsesUsage::from);
        // output mapping
        let output = resp
            .choices
            .iter()
            .enumerate()
            .map(|(i, choice)| {
                let msg = &choice.message;
                // If there are tool_calls, produce FunctionCall, else Message
                if let Some(tool_calls) = &msg.tool_calls {
                    // Take the first tool_call for mapping
                    let tc = &tool_calls[0];
                    Output::FunctionCall(OutputFunctionCall {
                        id: tc.id.clone().unwrap_or_default(),
                        arguments: tc.function.arguments.clone(),
                        // arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
                        call_id: msg.tool_call_id.clone().unwrap_or_default(),
                        name: tc.function.name.clone(),
                        status: ToolStatus::Completed,
                    })
                } else {
                    // Reasoning: if role is assistant and content is present, treat as Message, else Reasoning variant
                    Output::Message(OutputMessage {
                        id: format!("{}-{}", resp.id, i),
                        role: OutputRole::Assistant,
                        status: ResponseStatus::Completed,
                        content: vec![match &msg.content {
                            Some(content) => AssistantContent::OutputText(Text {
                                text: content.clone(),
                            }),
                            None => AssistantContent::Refusal {
                                refusal: "No content".to_string(),
                            },
                        }],
                    })
                }
            })
            .collect();
        CompletionResponse {
            id: resp.id,
            object: ResponseObject::Response,
            created_at: resp.created.unwrap_or_default(),
            status: ResponseStatus::Completed,
            error: None,
            incomplete_details: None,
            instructions: None,
            max_output_tokens: None,
            model: resp.model,
            usage,
            output,
            tools: {
                let mut tool_defs = Vec::new();
                for choice in &resp.choices {
                    if let Some(tool_calls) = &choice.message.tool_calls {
                        for tc in tool_calls {
                            tool_defs.push(ResponsesToolDefinition {
                                name: tc.function.name.clone(),
                                parameters: serde_json::from_str(&tc.function.arguments)
                                    .unwrap_or_default(),
                                strict: true,
                                kind: tc.tool_type.clone(),
                                description: String::new(),
                            });
                        }
                    }
                }
                tool_defs
            },
            additional_parameters: AdditionalParameters::default(),
        }
    }
}

impl From<CopilotUsage> for ResponsesUsage {
    fn from(u: CopilotUsage) -> Self {
        ResponsesUsage {
            input_tokens: u.prompt_tokens as u64,
            input_tokens_details: None,
            output_tokens: u.completion_tokens as u64,
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 0,
            },
            total_tokens: u.total_tokens as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::responses::models::prompt_request::PromptRequest;
    use serde_json;

    #[test]
    fn test_rig_openai_prompt_request_with_tools_result() {
        let json = include_str!("../resources/rig_openai_prompt_request_with_tools_result.json");
        let prompt_request: PromptRequest =
            serde_json::from_str(json).expect("Failed to parse PromptRequest");

        let copilot_request: CopilotChatRequest = prompt_request.into();
        let openai_completion_to_copilot_request =
            serde_json::to_string_pretty(&copilot_request).unwrap();

        println!("{}", openai_completion_to_copilot_request);

        assert_eq!(
            openai_completion_to_copilot_request,
            include_str!(
                "../resources/rig_openai_prompt_request_with_tools_result_as_copilot_request.json"
            )
        );
    }

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
