use std::collections::BTreeSet;

use piko_protocol::Message;

use crate::{Result, SessionAggregate, StoreError};

impl SessionAggregate {
    pub fn active_root_transcript(&self) -> Result<Vec<Message>> {
        let mut current = self.root_base_message_id.clone().or_else(|| {
            self.root
                .as_ref()
                .and_then(|root| self.agent_heads.get(&root.agent_instance_id).cloned())
        });
        let mut path = Vec::new();
        let mut visited = BTreeSet::new();
        while let Some(id) = current {
            if !visited.insert(id.clone()) {
                return Err(StoreError::InvalidEvent(format!(
                    "message ancestry cycle at {id}"
                )));
            }
            let message = self
                .messages
                .get(&id)
                .ok_or_else(|| StoreError::InvalidEvent(format!("missing active message {id}")))?;
            path.push(message.data.message.clone());
            current = message.data.agent_parent_message_id.clone();
        }
        path.reverse();
        Ok(path)
    }
}
