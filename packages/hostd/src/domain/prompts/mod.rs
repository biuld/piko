pub mod skills;

mod build;
mod template;
mod types;

pub use build::{assemble_agent_run_prompt, resolved_catalog, snapshot_prompt_resources};
pub use template::expand_prompt_template;
pub(crate) use template::parse_frontmatter_result;
pub use types::{ContextFile, PromptResourceError, PromptSnapshotOptions, PromptTemplate};
