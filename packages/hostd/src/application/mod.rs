//! Application layer: use-case services orchestrating `domain` policy through
//! `ports` (implemented by `adapters`).
//!
//! Dependency rule: `application` must not
//! `use crate::protocol`. `protocol` depends on `application`, never the
//! other way around.

pub mod compaction;
pub mod host_app;
pub mod sessions;
pub mod turns;

pub use host_app::HostApp;
