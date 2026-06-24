// orchd — piko orchestrator daemon library
//
// Re-exports all protocol types and orchestrator facade modules.

// The pedantic clippy lints below would require significant refactoring
// (Box'ing enum variants changes serde behaviour, splitting argument lists
// would obscure the 1:1 mapping with TS source). Suppress for now.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod actors;
pub mod model;
pub mod orchestrator;
pub mod protocol;
pub mod rpc;
pub mod tools;
