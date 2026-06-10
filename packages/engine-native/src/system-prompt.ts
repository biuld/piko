import type { EngineToolInfo } from "piko-engine-protocol";

interface BuildNativeSystemPromptOptions {
  cwd: string;
  tools: EngineToolInfo[];
  customPrompt?: string;
}

export function buildNativeSystemPrompt(options: BuildNativeSystemPromptOptions): string {
  const { tools, customPrompt, cwd } = options;

  const toolList =
    tools.length > 0
      ? tools.map((t) => `- ${t.name}: ${t.description}`).join("\n")
      : "- (no tools configured)";

  const guidelines: string[] = [
    "Be concise and technical.",
    "Use available tools instead of guessing about the workspace.",
    "Show file paths clearly when referring to code.",
  ];

  const toolNames = new Set(tools.map((t) => t.name));

  if (toolNames.has("shell")) {
    guidelines.push(
      "Use shell (cat, rg, fd, ls, find) for reading and exploring the workspace before making edits.",
    );
  }
  if (toolNames.has("apply_patch")) {
    guidelines.push(
      "Use apply_patch for all file edits. Write complete patches with *** Begin Patch / *** End Patch grammar.",
    );
  }

  const normalizedCwd = cwd.replace(/\\/g, "/");
  const now = new Date();
  const date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;

  if (customPrompt) {
    return `${customPrompt}\nCurrent date: ${date}\nCurrent working directory: ${normalizedCwd}`;
  }

  return `You are an expert coding assistant operating inside piko, a coding agent harness. You help users by reading files, executing commands, and applying patches to code.

You have access to two core tools:
- \`shell\` — run any shell command (use cat, rg, fd, ls, find for reading; use git, npm, etc. for operations)
- \`apply_patch\` — apply structured file patches with the *** Begin Patch / *** End Patch grammar

Available tools:
${toolList}

Guidelines:
${guidelines.map((g) => `- ${g}`).join("\n")}

Current date: ${date}
Current working directory: ${normalizedCwd}`;
}
