use futures::stream::BoxStream;
use orchd::protocol::messages::{Message, Model};

pub struct ChatRequest {
    pub model: Model,
    pub messages: Vec<Message>,
    pub temperature: f32,
}

pub struct ChatChunk {
    // We will define this properly to match self-llm or what orchd expects
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API error: {0}")]
    ApiError(String),
}

#[async_trait::async_trait]
pub trait LlmGateway: Send + Sync {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, LlmError>>, LlmError>;
}
