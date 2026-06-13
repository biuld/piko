import type { ToolInfo } from "piko-protocol";
import {
  buildSystemPrompt,
  loadContextFiles,
  loadPromptTemplates,
  type PromptTemplate,
} from "../prompts/index.js";
import { loadSkills } from "../skills/index.js";

export function buildEnhancedSystemPromptEngines(
  tools: ToolInfo[],
  cwd: string,
  appendSystemPrompt?: string,
  promptGuidelines?: string[],
  promptTemplates?: PromptTemplate[],
  skipContextFiles?: boolean,
): string {
  const toolSnippets: Record<string, string> = {};
  for (const t of tools) toolSnippets[t.name] = t.description;

  const contextFiles = skipContextFiles ? [] : loadContextFiles({ cwd });
  const skills = loadSkills({ cwd });
  const templates = promptTemplates ?? loadPromptTemplates({ cwd });

  return buildSystemPrompt({
    cwd,
    selectedTools: tools.map((t) => t.name),
    toolSnippets,
    contextFiles,
    skills: skills.skills,
    promptGuidelines,
    appendSystemPrompt,
    promptTemplates: templates.length > 0 ? templates : undefined,
  });
}
