//! Standard OpenAI `/responses` API — re-exports of shared types.
//!
//! The OpenAI Responses streaming format is identical to Copilot's.
//! Shared output/annotation/usage types are re-exported from
//! `copilot::responses::response` for use by other modules.

#[allow(unused_imports)]
pub use crate::copilot::responses::response::{
    Annotation, CodeInterpreterCallItem, CodeInterpreterOutput, ComputerCallItem,
    FileSearchCallItem, FileSearchResult, FunctionCallOutputItem, ImageGenerationCallItem,
    IncompleteDetails, InputTokensDetails, LocalShellAction, LocalShellCallItem, LogprobEntry,
    MessageOutputItem, OutputItem, OutputTextKind, OutputTextPart, OutputTokensDetails,
    ReasoningOutputItem, SummaryTextKind, SummaryTextPart, TopLogprob, Usage, WebSearchCallItem,
};
