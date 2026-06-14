/**
 * Skills types for piko.
 *
 * Follows the Agent Skills spec: https://agentskills.io/integrate-skills
 */

/** Parsed YAML frontmatter from a SKILL.md or .md skill file. */
export interface SkillFrontmatter {
  name?: string;
  description?: string;
  "disable-model-invocation"?: boolean;
  /** Provider/model string to switch to when this skill is invoked (e.g., "openai/gpt-4o"). */
  model?: string;
  /** Thinking level to use when this skill is invoked ("off", "low", "medium", "high"). */
  thinking?: string;
  /** Comma-separated list of tool names that should be active for this skill. */
  tools?: string | string[];
  [key: string]: unknown;
}

/** A loaded skill. */
export interface Skill {
  /** Unique skill name (lowercase a-z, 0-9, hyphens). */
  name: string;
  /** Short description of what the skill does. */
  description: string;
  /** Absolute path to the skill markdown file. */
  filePath: string;
  /** Directory containing the skill file. */
  baseDir: string;
  /** Whether automatic model invocation is disabled. */
  disableModelInvocation: boolean;
  /** If set, the model to switch to when this skill is invoked (provider/model). */
  modelOverride?: string;
  /** If set, the thinking level to use when this skill is invoked. */
  thinkingLevel?: string;
  /** If set, comma-separated list of active tool names for this skill. */
  activeTools?: string;
}
