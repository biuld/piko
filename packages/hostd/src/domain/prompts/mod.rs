pub mod skills;

mod build;
mod template;
mod types;

pub use build::{assemble_agent_run_prompt, build_system_prompt, snapshot_prompt_resources};
pub use template::expand_prompt_template;
pub(crate) use template::parse_frontmatter_result;
pub use types::{BuildSystemPromptOptions, ContextFile, PromptResourceError, PromptTemplate};
