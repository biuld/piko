//! Desktop state: the client-core projection plus shell-local status.

use piko_client_core::ClientState;

use crate::connection::DesktopConnection;

pub struct DesktopState {
    /// Sole host projection store (D-34 Slice 3b).
    pub core: ClientState,
    /// Observable shell connection state (F-42).
    pub connection: DesktopConnection,
    /// Human-readable status line for the shell surface.
    pub status: String,
}

impl DesktopState {
    pub fn new() -> Self {
        Self {
            core: ClientState::default(),
            connection: DesktopConnection::Connecting,
            status: "connecting to hostd".to_string(),
        }
    }

    pub fn on_spawned(&mut self) {
        self.connection = self.connection.on_spawned();
        self.status = "hydrating from hostd".to_string();
    }

    pub fn on_spawn_failure(&mut self, detail: String) {
        self.connection = self.connection.on_closed();
        self.status = format!("failed to spawn hostd: {detail}");
    }

    pub fn on_decode_error(&mut self, detail: String) {
        self.connection = self.connection.on_decode_error();
        self.status = format!("decode error: {detail}");
    }

    pub fn on_closed(&mut self) {
        self.connection = self.connection.on_closed();
        self.status = "hostd closed the connection".to_string();
    }

    pub fn on_send_failure(&mut self, detail: String) {
        self.connection = self.connection.on_send_failure();
        self.status = format!("send failed: {detail}");
    }

    pub fn on_host_message(&mut self) {
        self.connection = self.connection.on_message();
        if self.connection == DesktopConnection::Hydrating {
            self.status = "hydrating from hostd".to_string();
        }
    }

    pub fn on_hydrated(&mut self) {
        self.connection = self.connection.on_hydrated();
        self.status = "live".to_string();
    }

    /// Session inventory size; kept for tests and future chrome that needs it.
    #[allow(dead_code)]
    pub fn session_count(&self) -> usize {
        self.core.session_list.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_failure_reaches_disconnected() {
        let mut state = DesktopState::new();
        state.on_spawn_failure("no binary".to_string());
        assert_eq!(state.connection, DesktopConnection::Disconnected);
    }

    #[test]
    fn hydration_status_clears_only_when_bootstrap_finishes() {
        let mut state = DesktopState::new();
        state.on_spawned();
        assert_eq!(state.connection, DesktopConnection::Hydrating);
        state.on_host_message();
        state.on_hydrated();
        assert_eq!(state.connection, DesktopConnection::Live);
        assert_eq!(state.status, "live");
    }

    #[test]
    fn decode_error_reports_but_keeps_core() {
        let mut state = DesktopState::new();
        state.on_spawned();
        state.on_decode_error("bad line".to_string());
        assert_eq!(state.connection, DesktopConnection::DecodeError);
        state.on_host_message();
        assert_eq!(state.connection, DesktopConnection::Hydrating);
    }
}
