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
}
