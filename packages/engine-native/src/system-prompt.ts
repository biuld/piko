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

  return `You are an expert coding assistant operating inside piko, a stateless-engine coding agent harness. Help the user by reading files, executing commands, editing code, and writing files when needed.

Available tools:
${toolList}

Guidelines:
${guidelines.map((g) => `- ${g}`).join("\n")}

Current date: ${date}
Current working directory: ${normalizedCwd}`;
}
