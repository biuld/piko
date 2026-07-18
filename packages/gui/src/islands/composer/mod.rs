//! Composer island: Activity Center + input (Activity is Composer chrome).

mod activity_vm;
mod render;
mod view;
mod vm;

pub use activity_vm::{ActivityItem, derive_activity};
pub use view::ComposerIsland;
pub use vm::derive_composer;

#[cfg(test)]
pub(crate) use activity_vm::ActivityItemKind;
