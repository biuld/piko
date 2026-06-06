// Re-exports from piko-session — canonical message types and converters.
// Previously defined locally; now sourced from the shared session package.

export {
  type BashExecutionMessage,
  BRANCH_SUMMARY_PREFIX,
  BRANCH_SUMMARY_SUFFIX,
  type BranchSummaryMessage,
  bashExecutionToText,
  COMPACTION_SUMMARY_PREFIX,
  COMPACTION_SUMMARY_SUFFIX,
  type CompactionSummaryMessage,
  type CustomMessage,
  convertToLlm,
  createBranchSummaryMessage,
  createCompactionSummaryMessage,
  createCustomMessage,
} from "piko-session";
