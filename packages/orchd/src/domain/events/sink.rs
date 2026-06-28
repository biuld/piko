// ---- Domain: EventSink — abstraction for emitting host-facing events ----

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use piko_protocol::Event;

/// A sink that receives domain events and delivers them to the host layer.
///
/// This is a domain-level port: the domain defines the contract, and
/// adapters implement the delivery mechanism (e.g., stdio JSONL, WebSocket).
pub trait EventSink: Send + Sync {
    /// Emit an event to the host layer.
    fn emit(&self, event: Event) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// Type-erased event sink using an Arc'd closure.
pub type SharedEventSink = Arc<
    dyn Fn(Event) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
>;

impl EventSink for SharedEventSink {
    fn emit(&self, event: Event) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        (self)(event)
    }
}
