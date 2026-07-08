use llmd::gateway::GatewayEvent;

#[derive(Debug, Clone)]
pub struct ToolCallItem {
    pub content_index: u32,
    pub tool_call_index: u32,
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

struct InFlightToolCall {
    tool_call_index: u32,
    id: String,
    name: String,
    arguments_json: String,
}

pub struct ToolCallChunkUpdate {
    pub content_index: u32,
    pub tool_call_index: u32,
    pub tool_call_id: String,
    pub delta: String,
}

#[derive(Default)]
pub struct ToolCallAggregator {
    next_tool_call_index: u32,
    current: Option<InFlightToolCall>,
    completed: Vec<ToolCallItem>,
}

impl ToolCallAggregator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_gateway_event(&mut self, event: &GatewayEvent) -> Option<ToolCallChunkUpdate> {
        match event {
            GatewayEvent::ToolCallChunk {
                id,
                name,
                args_delta,
            } => self.on_chunk(id.clone(), name.clone(), args_delta.clone()),
            _ => None,
        }
    }

    pub fn on_chunk(
        &mut self,
        id: String,
        name: String,
        args_delta: String,
    ) -> Option<ToolCallChunkUpdate> {
        if !name.is_empty() {
            self.finalize_current();
            let tool_call_index = self.next_tool_call_index;
            self.next_tool_call_index += 1;
            self.current = Some(InFlightToolCall {
                tool_call_index,
                id: id.clone(),
                name,
                arguments_json: args_delta.clone(),
            });
            return Some(ToolCallChunkUpdate {
                content_index: tool_call_index,
                tool_call_index,
                tool_call_id: id,
                delta: args_delta,
            });
        }

        let current = self.current.as_mut()?;
        current.arguments_json.push_str(&args_delta);
        Some(ToolCallChunkUpdate {
            content_index: current.tool_call_index,
            tool_call_index: current.tool_call_index,
            tool_call_id: current.id.clone(),
            delta: args_delta,
        })
    }

    pub fn flush(&mut self) -> Vec<ToolCallItem> {
        self.finalize_current();
        std::mem::take(&mut self.completed)
    }

    fn finalize_current(&mut self) {
        let Some(current) = self.current.take() else {
            return;
        };

        let arguments = match serde_json::from_str::<serde_json::Value>(&current.arguments_json) {
            Ok(arguments) => arguments,
            Err(_) => serde_json::Value::String(current.arguments_json),
        };

        self.completed.push(ToolCallItem {
            content_index: current.tool_call_index,
            tool_call_index: current.tool_call_index,
            id: current.id,
            name: current.name,
            arguments,
        });
    }
}
