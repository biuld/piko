use super::*;

impl ExecutionActor {
    pub(super) fn transition(&mut self, status: ExecutionStatus) {
        self.state.status = status;
    }

    pub(super) fn drain_controls_nonblocking(&mut self) -> Result<(), AgentApiError> {
        while let Ok(command) = self.mailbox.try_recv() {
            self.handle_command(command)?;
        }
        Ok(())
    }

    pub(super) async fn drain_controls_at_step_boundary(&mut self) -> Result<(), AgentApiError> {
        self.drain_controls_nonblocking()?;
        Ok(())
    }

    pub(super) fn handle_command(
        &mut self,
        command: ExecutionCommand,
    ) -> Result<(), AgentApiError> {
        match command {
            ExecutionCommand::Steer { request, reply } => {
                let command = ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                let receipt = ExecutionInputReceipt {
                    request_id: request.request_id.clone(),
                    session_id: self.identity.session_id.clone(),
                    root_input_id: self.identity.root_input_id.clone(),
                    message_id: request.message_id.clone(),
                    disposition: InputDisposition::Queued,
                };
                self.state.steering.push_back(request);
                command.complete(Ok(receipt));
            }
            ExecutionCommand::Cancel {
                request_id,
                reason: _,
                reply,
            } => {
                let command = ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                self.cancel.cancel();
                command.complete(Ok(CancelReceipt {
                    request_id,
                    session_id: self.identity.session_id.clone(),
                    root_input_id: self.identity.root_input_id.clone(),
                    accepted: true,
                }));
            }
            ExecutionCommand::Shutdown { reply } => {
                self.cancel.cancel();
                let _ = reply.send(());
            }
        }
        Ok(())
    }
}
