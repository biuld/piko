use async_trait::async_trait;

#[derive(Default)]
pub(super) struct HostPromptAssemblyPort;

#[async_trait]
impl orchd_api::PromptAssemblyPort for HostPromptAssemblyPort {
    async fn assemble_prompt(
        &self,
        request: piko_protocol::PromptAssemblyRequest,
    ) -> Result<piko_protocol::AgentRunPrompt, orchd_api::AgentApiError> {
        Ok(crate::domain::prompts::assemble_agent_run_prompt(&request))
    }
}
