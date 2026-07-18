//! UUID-based command id source for production use.

use piko_client_core::CommandIdSource;
use uuid::Uuid;

/// Generates unique command ids using UUID v4.
pub struct UuidCommandIdSource;

impl CommandIdSource for UuidCommandIdSource {
    fn next_command_id(&mut self) -> String {
        Uuid::new_v4().to_string()
    }
}
