import type { PromptTemplate } from "../prompts/index.js";
import { substituteArgs } from "../prompts/index.js";
import type { Skill } from "../skills/index.js";

/** Format a skill as a prompt string. Exported for TUI use. */
export function formatSkillPrompt(
  skill: { name: string; filePath: string; description: string },
  additionalInstructions?: string,
): string {
  let prompt = `Read and follow the skill at @${skill.filePath}: ${skill.description}`;
  if (additionalInstructions) {
    prompt += `\n\nAdditional instructions: ${additionalInstructions}`;
  }
  return prompt;
}

/** Format a skill invocation prompt from loaded skills lookup. */
export function buildSkillPrompt(
  skills: Skill[],
  name: string,
  additionalInstructions?: string,
): string {
  const skill = skills.find((s) => s.name === name);
  if (!skill) throw new Error(`Unknown skill: ${name}`);
  return formatSkillPrompt(skill, additionalInstructions);
}

/** Build the prompt for a template invocation. */
export function buildTemplatePrompt(
  templates: PromptTemplate[],
  name: string,
  args: string[],
): string {
  const template = templates.find((t) => t.name === name);
  if (!template) throw new Error(`Unknown prompt template: ${name}`);
  const expanded = substituteArgs(template.content, args);
  return `Run template /${name}: ${expanded}`;
}
