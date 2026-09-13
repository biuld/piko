// ---- Adapters: concrete implementations of ports ----
//
// Adapters implement the port interfaces and wire them to real
// infrastructure (tool registries, host event bridges).

pub mod persist;
pub mod tools;
