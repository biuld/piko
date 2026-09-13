//! Compatibility re-export of the shared wire contract.
//!
//! This module is a full re-export of `piko_protocol`. It exists so the
//! crate can refer to the wire contract as `crate::api` uniformly; it must
//! never grow hostd-owned types. New DTOs belong in `piko-protocol` (see
//! `packages/protocol/AGENTS.md`) — if you are about to add a type here,
//! add it there instead.

pub use piko_protocol::*;
