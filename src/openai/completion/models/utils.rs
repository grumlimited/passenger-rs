use crate::copilot::models::{CopilotModel, CopilotModelsResponse};
use crate::openai::completion::models::{
    OpenAIChatRequest, OpenAIMessage, OpenAIModel, OpenAIModelsResponse,
};
impl OpenAIChatRequest {
    fn assistant_role() -> String {
        "assistant".to_string()
    }

    fn tool_role() -> String {
        "tool".to_string()
    }

    fn has_valid_id(id: &Option<String>) -> bool {
        id.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Checks if all tool-related messages already have IDs present.
    /// Returns true if IDs are present, meaning the initial payload included them
    /// and we should not modify them. Returns false if any IDs are missing,
    /// indicating we need to generate them via ensure_tool_ids().
    pub fn ids_present(&self) -> bool {
        let all_tool_messages_have_ids = self
            .messages
            .iter()
            .filter(|t| t.role == Self::tool_role())
            .all(|msg| Self::has_valid_id(&msg.tool_call_id));

        let all_tool_calls_have_ids = self
            .messages
            .iter()
            .filter(|msg| msg.role == Self::assistant_role())
            .filter_map(|msg| msg.tool_calls.as_ref())
            .flat_map(|calls| calls.iter())
            .all(|call| Self::has_valid_id(&call.id));

        all_tool_messages_have_ids && all_tool_calls_have_ids
    }

    /// Applies all necessary transformations for GitHub Copilot compatibility.
    ///
    /// This is the main entry point for preparing requests before sending to Copilot.
    /// It orchestrates two critical transformations:
    /// 1. Ensures tool IDs are present (required by OpenAI spec)
    /// 2. Duplicates tool messages as user messages (works around Copilot quirks)
    ///
    /// Call this method once on any request that contains tools before forwarding to Copilot.
    pub fn prepare_for_copilot(&mut self) {
        self.ensure_tool_ids();
        // self.duplicate_tool_messages_as_user();
    }

    /// Generates and assigns IDs to tool-related messages when they are missing.
    /// This method only modifies the request if ids_present() returns false.
    ///
    /// It assigns:
    /// - tool_call_id to messages with role "tool" (indexed sequentially)
    /// - id to tool_calls in assistant messages (indexed sequentially)
    /// - name to tool messages (extracted from assistant's tool_calls)
    ///
    /// If the original request already had IDs, this method does nothing,
    /// preserving the client-provided identifiers.
    ///
    /// # Why This Is Necessary
    ///
    /// This normalization is required because different API providers have different requirements:
    /// - **Ollama API**: Does not include tool_call_id or id fields in its specification
    /// - **OpenAI API**: Requires these IDs for proper tool calling workflow
    /// - **GitHub Copilot**: Follows OpenAI's standard and expects IDs to be present
    ///
    /// When using frameworks like [Rig](https://github.com/0xPlaygrounds/rig) with its Ollama provider,
    /// the generated OpenAIChatRequest structs won't have these IDs. This proxy bridges
    /// that gap by auto-generating them before forwarding to GitHub Copilot.
    fn ensure_tool_ids(&mut self) {
        if !self.ids_present() {
            let assistant_tool_name = self
                .messages
                .iter()
                .filter(|message| message.role == Self::assistant_role())
                .flat_map(|message| match &message.tool_calls {
                    Some(tool_calls) => tool_calls.clone(),
                    _ => Vec::new(),
                })
                .map(|tool_call| tool_call.function.name)
                .collect::<Vec<String>>();

            self.messages
                .iter_mut()
                .filter(|message| message.role == Self::tool_role())
                .enumerate()
                .zip(assistant_tool_name.iter())
                .for_each(|((idx, message), tool_name)| {
                    message.name = Some(tool_name.to_string());
                    message.tool_call_id = Some(format!("{}", idx))
                });

            self.messages
                .iter_mut()
                .filter(|message| message.role == Self::assistant_role())
                .filter(|message| message.tool_calls.is_some())
                .for_each(|message| {
                    if let Some(ref mut tc) = message.tool_calls {
                        tc.iter_mut().enumerate().for_each(|(idx, tool_call)| {
                            tool_call.id = Some(format!("{}", idx));
                        })
                    }
                });
        }
    }

    /// Duplicates tool messages as user messages for GitHub Copilot compatibility.
    ///
    /// GitHub Copilot validates that `tool_calls` in assistant messages have corresponding
    /// `role: "tool"` messages with matching IDs. However, when `role: "tool"` messages are
    /// present, Copilot sometimes returns empty choices arrays (intermittent behavior).
    ///
    /// This method works around both constraints by:
    /// 1. Keeping the original `role: "tool"` messages in place (for validation)
    /// 2. Appending `role: "user"` message duplicates after the last tool message
    ///    (for the LLM to actually read and process)
    ///
    /// # Message Flow
    ///
    /// The method preserves the natural message ordering that Copilot expects:
    /// - `assistant` message with `tool_calls`
    /// - All corresponding `tool` messages (grouped together)
    /// - User message summaries (appended at the end)
    ///
    /// Original:
    /// ```json
    /// [
    ///   {"role": "assistant", "tool_calls": [{"id": "call_123", ...}]},
    ///   {"role": "tool", "tool_call_id": "call_123", "name": "get_weather", "content": "{\"temperature\": 72}"}
    /// ]
    /// ```
    ///
    /// After duplication:
    /// ```json
    /// [
    ///   {"role": "assistant", "tool_calls": [{"id": "call_123", ...}]},
    ///   {"role": "tool", "tool_call_id": "call_123", "name": "get_weather", "content": "{\"temperature\": 72}"},
    ///   {"role": "user", "content": "Tool 'get_weather' (call_123) returned: {\"temperature\": 72}"}
    /// ]
    /// ```
    ///
    /// This approach trades token consumption for reliability, ensuring Copilot both
    /// validates the tool calling chain AND consistently processes the results.
    fn _duplicate_tool_messages_as_user(&mut self) {
        let mut user_duplicates = Vec::new();
        let mut last_tool_index = None;

        // Find all tool messages and create user message duplicates
        for (idx, message) in self.messages.iter().enumerate() {
            if message.role == Self::tool_role() {
                last_tool_index = Some(idx);

                let tool_name = message.name.as_deref().unwrap_or("unknown_tool");
                let tool_call_id = message.tool_call_id.as_deref().unwrap_or("unknown_id");
                let original_content = message.content.as_deref().unwrap_or("");

                // Create a user message with formatted tool result
                let user_message = OpenAIMessage {
                    role: "user".to_string(),
                    content: Some(format!(
                        "Tool '{}' ({}) returned: {}",
                        tool_name, tool_call_id, original_content
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                };

                user_duplicates.push(user_message);
            }
        }

        // Insert all user duplicates after the last tool message
        if let Some(insert_pos) = last_tool_index {
            // Insert in reverse order to maintain correct final ordering
            for user_msg in user_duplicates.into_iter().rev() {
                self.messages.insert(insert_pos + 1, user_msg);
            }
        }
    }
}

impl From<CopilotModelsResponse> for OpenAIModelsResponse {
    fn from(value: CopilotModelsResponse) -> Self {
        Self {
            data: value.models.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<CopilotModel> for OpenAIModel {
    fn from(value: CopilotModel) -> Self {
        Self {
            id: value.id,
            object: "model".to_string(),
            created: 1687882411,
            owned_by: value.family,
        }
    }
}
