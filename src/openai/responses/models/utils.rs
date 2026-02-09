use std::fmt::Display;
use crate::openai::responses::models::prompt_response::OutputRole;

impl Display for OutputRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            OutputRole::Assistant => "assistant".to_string(),
        })
    }
}
