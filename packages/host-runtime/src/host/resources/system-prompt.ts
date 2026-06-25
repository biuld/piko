import type { ToolInfo } from "piko-orch-protocol";
import {
  buildSystemPrompt,
  loadContextFiles,
  loadPromptTemplates,
  type PromptTemplate,
} from "../../prompts/index.js";
import { loadSkills } from "../../skills/index.js";

export async function buildEnhancedSystemPromptEngines(
  tools: ToolInfo[],
  cwd: string,
  appendSystemPrompt?: string,
  promptGuidelines?: string[],
  promptTemplates?: PromptTemplate[],
  skipContextFiles?: boolean,
): Promise<string> {
  const toolSnippets: Record<string, string> = {};
  for (const t of tools) toolSnippets[t.name] = t.description;

  const contextFiles = skipContextFiles ? [] : await loadContextFiles({ cwd });
  const skills = await loadSkills({ cwd });
  const templates = promptTemplates ?? (await loadPromptTemplates({ cwd }));

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
