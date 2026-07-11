// ---- Adapters: concrete implementations of ports ----
//
// Adapters implement the port interfaces and wire them to real
// infrastructure (tool registries, LLM gateways, host event bridges).

pub mod model;
pub mod persist;
pub mod tools;
