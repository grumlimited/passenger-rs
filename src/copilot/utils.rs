use crate::copilot::{CopilotChatRequest, CopilotMessage};
use crate::openai::completion::models::OpenAIChatRequest;

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
