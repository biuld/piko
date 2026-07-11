// ---- Application: service facade and task supervision ----

pub(crate) mod commands;
pub(crate) mod queries;
pub mod service;
pub(crate) mod supervision;

pub use supervision::Supervisor;
