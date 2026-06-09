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

  // New toolset: shell + apply_patch
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

  // Legacy toolset compatibility
  if (toolNames.has("read")) {
    guidelines.push("Read relevant files before making non-trivial edits.");
  }
  if (toolNames.has("grep") || toolNames.has("find") || toolNames.has("ls")) {
    guidelines.push("Use grep, find, and ls for codebase exploration before broad changes.");
  } else if (toolNames.has("bash")) {
    guidelines.push(
      "Use bash for workspace exploration when dedicated read-only tools are unavailable.",
    );
  }
  if (toolNames.has("edit") || toolNames.has("write")) {
    guidelines.push(
      "When changing code, preserve the existing structure unless the task requires a larger refactor.",
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
