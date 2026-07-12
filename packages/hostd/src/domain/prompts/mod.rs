pub mod skills;

mod build;
mod template;
mod types;

pub use build::build_system_prompt;
pub use template::expand_prompt_template;
pub(crate) use template::parse_frontmatter_result;
pub use types::{
    BuildSystemPromptOptions, ContextFile, PromptResourceError, PromptResources, PromptTemplate,
};
