use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use crate::middleware::{GatewayContext, LlmdMiddleware};

use crate::gateway::GatewayEvent;

#[derive(Default)]
pub struct TokenUsageMiddleware {
    pub total_input: AtomicU64,
    pub total_output: AtomicU64,
}

impl TokenUsageMiddleware {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl LlmdMiddleware for TokenUsageMiddleware {
    async fn on_stream_event(
        &self,
        ctx: &mut GatewayContext,
        event: &mut GatewayEvent,
    ) -> Result<(), String> {
        if let GatewayEvent::Usage(usage) = event {
            // Add to the global atomic counters
            let current_in = self.total_input.fetch_add(usage.input, Ordering::SeqCst);
            let current_out = self.total_output.fetch_add(usage.output, Ordering::SeqCst);

            // Emit a dedicated telemetry log for token usage
            info!(
                run_id = %ctx.run_id,
                step_id = %ctx.step_id,
                input_tokens = usage.input,
                output_tokens = usage.output,
                cache_read_tokens = usage.cache_read,
                gateway_total_input = current_in + usage.input,
                gateway_total_output = current_out + usage.output,
                "Token usage tracked"
            );

            // Expose the raw counts via the context metadata for other middlewares
            ctx.metadata
                .insert("input_tokens".to_string(), usage.input.to_string());
            ctx.metadata
                .insert("output_tokens".to_string(), usage.output.to_string());
            ctx.metadata.insert(
                "cache_read_tokens".to_string(),
                usage.cache_read.to_string(),
            );
            ctx.metadata.insert(
                "cache_write_tokens".to_string(),
                usage.cache_write.to_string(),
            );
        }
        Ok(())
    }
}
