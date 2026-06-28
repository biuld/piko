// ---- Port: EventStore — interface for persisting/forwarding events ----
//
// This re-exports the domain-level EventSink as a port, since event
// delivery is an external capability.

pub use crate::domain::events::sink::{EventSink, SharedEventSink};
