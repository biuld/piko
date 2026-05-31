import type { Command } from "./types.js";

export const COMMANDS: Command[] = [
  { value: "/help", label: "/help", description: "Show help" },
  { value: "/model", label: "/model [next|prev]", description: "Show or switch model" },
  { value: "/models", label: "/models", description: "List available models" },
  { value: "/sessions", label: "/sessions", description: "List saved sessions" },
  { value: "/import", label: "/import <path>", description: "Import a session JSONL file" },
  { value: "/name", label: "/name <title>", description: "Set the current session title" },
  { value: "/tree", label: "/tree [entry-id]", description: "Show or switch the current branch" },
  {
    value: "/fork",
    label: "/fork <entry-id>",
    description: "Create a new session from an earlier user message",
  },
  {
    value: "/clone",
    label: "/clone",
    description: "Duplicate the current branch into a new session",
  },
  { value: "/resume", label: "/resume <id>", description: "Resume a saved session" },
  { value: "/session", label: "/session", description: "Show current session info" },
  { value: "/new", label: "/new", description: "Start a new session" },
  { value: "/clear", label: "/clear", description: "Clear chat" },
  {
    value: "/thinking",
    label: "/thinking [off|low|medium|high|xhigh]",
    description: "Set thinking level",
  },
  {
    value: "/theme",
    label: "/theme [dark|light]",
    description: "Show or switch theme",
  },
  { value: "/login", label: "/login <provider>", description: "Set API key for a provider" },
  { value: "/settings", label: "/settings", description: "Open settings selector" },
  {
    value: "/template",
    label: "/template <name> [args...]",
    description: "Invoke a prompt template",
  },
  { value: "/skill", label: "/skill <name>", description: "Invoke a skill" },
  { value: "/compact", label: "/compact", description: "Compact the current session context" },
  { value: "/export", label: "/export [path]", description: "Export session to HTML" },
  { value: "/reload", label: "/reload", description: "Reload settings, skills and templates" },
  { value: "/logout", label: "/logout <provider>", description: "Remove stored API key for a provider" },
  { value: "/exit", label: "/exit", description: "Exit piko" },
];
