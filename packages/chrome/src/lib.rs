//! GPUI Islands chrome kit.
//!
//! ```text
//! src/
//!   runtime/      archipelago · island contracts · layout trees
//!   components/   panel · overlay · list/tree · Markdown
//!   theme/        tokens · metrics · typography · icons
//!   assets/       embedded SVG AssetSource
//! ```
//!
//! **Does not own** product archipelago/island ids, domain messages, backend
//! bridges, or application frame assembly.
//!
//! The public API follows this source layout directly. Runtime policy and
//! presentational components stay visibly separate at every call site.

pub mod assets;
pub mod components;
pub mod runtime;
pub mod theme;
