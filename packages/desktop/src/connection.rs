//! Desktop connection state machine.
//!
//! Richer than client-core's binary `ConnectionState`: the shell must expose
//! the F-42 observable states (connecting, hydrating, live, disconnected,
//! decode error) while client-core keeps its own transport observation
//! state. Transitions are pure so they can be unit-tested without GPUI.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DesktopConnection {
    /// Host process spawn is in flight.
    #[default]
    Connecting,
    /// Spawned; bootstrap commands sent, first projection not yet arrived.
    Hydrating,
    /// First host-authored projection arrived.
    Live,
    /// Host transport closed or spawn failed.
    Disconnected,
    /// A host line failed to decode; transport may still be open.
    DecodeError,
}

impl DesktopConnection {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Hydrating => "Hydrating",
            Self::Live => "Live",
            Self::Disconnected => "Disconnected",
            Self::DecodeError => "Decode error",
        }
    }

    /// The host process spawned and bootstrap is being sent.
    pub fn on_spawned(self) -> Self {
        match self {
            Self::Connecting => Self::Hydrating,
            other => other,
        }
    }

    /// A valid host message arrived. Decode errors recover to hydration;
    /// product code marks the connection live only after bootstrap completes.
    pub fn on_message(self) -> Self {
        match self {
            Self::DecodeError => Self::Hydrating,
            other => other,
        }
    }

    pub fn on_hydrated(self) -> Self {
        match self {
            Self::Hydrating => Self::Live,
            other => other,
        }
    }

    /// A line failed to decode.
    pub fn on_decode_error(self) -> Self {
        Self::DecodeError
    }

    /// The transport closed (or spawn failed).
    pub fn on_closed(self) -> Self {
        Self::Disconnected
    }

    /// A write to the host failed; the process is gone for practical
    /// purposes.
    pub fn on_send_failure(self) -> Self {
        Self::Disconnected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_connecting_to_live() {
        let mut connection = DesktopConnection::default();
        assert_eq!(connection, DesktopConnection::Connecting);
        connection = connection.on_spawned();
        assert_eq!(connection, DesktopConnection::Hydrating);
        connection = connection.on_hydrated();
        assert_eq!(connection, DesktopConnection::Live);
    }

    #[test]
    fn live_stays_live_across_messages() {
        assert_eq!(
            DesktopConnection::Live.on_message(),
            DesktopConnection::Live
        );
    }

    #[test]
    fn decode_error_is_visible_and_recovers_on_valid_message() {
        assert_eq!(
            DesktopConnection::Live.on_decode_error(),
            DesktopConnection::DecodeError
        );
        assert_eq!(
            DesktopConnection::DecodeError.on_message(),
            DesktopConnection::Hydrating
        );
    }

    #[test]
    fn closure_wins_from_any_state() {
        for connection in [
            DesktopConnection::Connecting,
            DesktopConnection::Hydrating,
            DesktopConnection::Live,
            DesktopConnection::DecodeError,
        ] {
            assert_eq!(
                connection.on_closed(),
                DesktopConnection::Disconnected,
                "{connection:?} must close to Disconnected"
            );
        }
    }

    #[test]
    fn send_failure_disconnects() {
        assert_eq!(
            DesktopConnection::Live.on_send_failure(),
            DesktopConnection::Disconnected
        );
    }
}
