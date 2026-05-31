import type { Command } from "./types.js";

/**
 * Slash command definitions — aligned with pi's BUILTIN_SLASH_COMMANDS.
 *
 * pi reference: packages/coding-agent/src/core/slash-commands.ts
 */
export const COMMANDS: Command[] = [
  { value: "/settings", label: "/settings", description: "Open settings menu" },
  { value: "/model", label: "/model [search]", description: "Select model (opens selector UI)" },
  {
    value: "/scoped-models",
    label: "/scoped-models",
    description: "Enable/disable models for cycling",
  },
  {
    value: "/export",
    label: "/export [path]",
    description: "Export session (HTML default, or .jsonl)",
  },
  {
    value: "/import",
    label: "/import <path>",
    description: "Import and resume a session from a JSONL file",
  },
  { value: "/share", label: "/share", description: "Share session as a secret GitHub gist" },
  { value: "/copy", label: "/copy", description: "Copy last agent message to clipboard" },
  { value: "/name", label: "/name [title]", description: "Set session display name" },
  { value: "/session", label: "/session", description: "Show session info and stats" },
  { value: "/changelog", label: "/changelog", description: "Show changelog entries" },
  { value: "/hotkeys", label: "/hotkeys", description: "Show all keyboard shortcuts" },
  { value: "/fork", label: "/fork", description: "Create a new fork from a previous user message" },
  {
    value: "/clone",
    label: "/clone",
    description: "Duplicate the current session at the current position",
  },
  { value: "/tree", label: "/tree", description: "Navigate session tree (switch branches)" },
  { value: "/login", label: "/login [provider]", description: "Configure provider authentication" },
  { value: "/logout", label: "/logout <provider>", description: "Remove provider authentication" },
  { value: "/new", label: "/new", description: "Start a new session" },
  {
    value: "/compact",
    label: "/compact [instructions]",
    description: "Manually compact the session context",
  },
  { value: "/resume", label: "/resume", description: "Resume a different session" },
  {
    value: "/reload",
    label: "/reload",
    description: "Reload keybindings, extensions, skills, prompts, and themes",
  },
  { value: "/quit", label: "/quit", description: "Quit piko" },
];
