//! Piko visual theme — chrome kit re-exports + product domain accents.
//!
//! Prefer `island` for chrome-only code. This module keeps existing
//! `crate::theme::…` import paths working and owns domain role colors that must
//! not live in chrome core.

mod domain;

pub use domain::{DomainRole, domain_role_hsla, domain_role_rgba};
pub use island::theme::*;
