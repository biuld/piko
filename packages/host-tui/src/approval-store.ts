// ============================================================================
// ApprovalStore — scoped tool approval storage with per-tool fingerprinting.
//
// Three scopes:
// - session:   in-memory, cleared when the TUI session ends
// - workspace: .piko/approvals.json (project-level)
// - permanent: ~/.piko/approvals.json (global)
//
// Instead of storing approvals by raw toolName, each call is reduced to a
// "fingerprint" — a tool-specific summary of what makes two calls "the same".
// For bash this means extracting the CLI program (or full command for unknowns).
// For edit/write/read it's path-based; other tools fall back to toolName.
// ============================================================================

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { joinPath } from "piko-host-runtime";

export type ApprovalScope = "session" | "workspace" | "permanent";

interface StoredApproval {
  toolName: string;
  fingerprint: string;
  /** The args that were used when this approval was granted (for reference). */
  sampleArgs?: Record<string, unknown>;
  grantedAt: number;
}

interface ApprovalsFile {
  fingerprints: Record<string, StoredApproval>;
}

// ============================================================================
// Fingerprint strategies
// ============================================================================

/**
 * Wrapper commands that precede the real program.
 * Pattern: skip the wrapper, skip its flags/assignments, find the real program.
 */
const BASH_WRAPPERS = new Set([
  "sudo",
  "env",
  "nice",
  "nohup",
  "time",
  "flock",
  "chroot",
  "su",
  "doas",
]);

/**
 * Known CLI programs — all sub-operations treated as the same class.
 * e.g. "git status" and "git push" share fingerprint "bash:git".
 */
const BASH_PROGRAM_LEVEL = new Set([
  "git",
  "ls",
  "cat",
  "find",
  "grep",
  "rg",
  "fd",
  "curl",
  "wget",
  "ssh",
  "scp",
  "docker",
  "kubectl",
  "python",
  "python3",
  "node",
  "tsc",
  "biome",
  "eslint",
  "prettier",
  "make",
  "cmake",
  "echo",
  "printf",
  "date",
  "which",
  "whoami",
  "uname",
  "head",
  "tail",
  "wc",
  "sort",
  "uniq",
  "cut",
  "tr",
  "awk",
  "sed",
  "xargs",
  "tee",
  "diff",
  "patch",
  "zip",
  "unzip",
  "tar",
  "ps",
  "df",
  "du",
  "ping",
  "nslookup",
  "dig",
]);

/**
 * Package managers — subcommand matters (install ≠ publish).
 * Fingerprint includes the first subcommand: "bash:npm:install".
 */
const BASH_PACKAGE_MANAGERS = new Set([
  "npm",
  "yarn",
  "pnpm",
  "bun",
  "pip",
  "pip3",
  "poetry",
  "brew",
  "apt",
  "apt-get",
  "dnf",
  "pacman",
  "snap",
  "gem",
  "cargo",
  "go",
  "composer",
  "rustup",
  "dotnet",
]);

interface ParsedBashCommand {
  program: string;
  subcommand?: string;
}

/**
 * Parse a bash command string into program + optional subcommand.
 * Skips known wrappers (sudo, env, ...) and flag/assignment tokens.
 * Returns null if the command is empty or unparseable.
 */
function parseBashCommand(command: string): ParsedBashCommand | null {
  if (!command?.trim()) return null;

  const tokens = command.trim().split(/\s+/);
  let i = 0;

  // Skip wrapper prefixes and their flags/assignments.
  // After a wrapper, skip flag tokens (start with '-') and assignments (contain '='),
  // and skip one value token after each flag.
  while (i < tokens.length && BASH_WRAPPERS.has(tokens[i])) {
    i++; // skip wrapper
    // Skip flag + its value, and assignments
    while (i < tokens.length) {
      const tok = tokens[i];
      if (tok.startsWith("-") && tok.length > 1) {
        // Long flag with '=' like --user=root → skip it, don't skip next
        if (tok.includes("=")) {
          i++;
          continue;
        }
        // Flag with separate value → skip both
        i++; // skip flag
        if (i < tokens.length && !tokens[i].startsWith("-") && !tokens[i].includes("=")) {
          i++; // skip value
        }
        continue;
      }
      if (tok.includes("=")) {
        i++;
        continue;
      }
      break; // found a non-flag, non-assignment token → it's the real program
    }
  }

  // Skip leading flags/assignments before the real program
  while (i < tokens.length && (tokens[i].startsWith("-") || tokens[i].includes("="))) {
    i++;
  }

  if (i >= tokens.length) return null;

  const program = tokens[i];

  // Package manager → find subcommand
  if (BASH_PACKAGE_MANAGERS.has(program)) {
    let j = i + 1;
    // Skip flags between program and subcommand
    while (j < tokens.length && (tokens[j].startsWith("-") || tokens[j].includes("="))) {
      j++;
    }
    if (j < tokens.length) {
      return { program, subcommand: tokens[j] };
    }
    return { program };
  }

  // Known program → program-level only
  if (BASH_PROGRAM_LEVEL.has(program)) {
    return { program };
  }

  // Unknown program — return it, caller will decide to fallback
  return { program };
}

/**
 * Compute the fingerprint for a bash tool call.
 */
export function computeBashFingerprint(args: Record<string, unknown>): string {
  const command = typeof args.command === "string" ? args.command : "";
  if (!command.trim()) return "bash";

  const parsed = parseBashCommand(command);
  if (!parsed) return "bash";

  const { program, subcommand } = parsed;

  // Known program → program-level fingerprint
  if (BASH_PROGRAM_LEVEL.has(program)) {
    return `bash:${program}`;
  }

  // Package manager → program:subcommand fingerprint
  if (BASH_PACKAGE_MANAGERS.has(program)) {
    if (subcommand) return `bash:${program}:${subcommand}`;
    return `bash:${program}`;
  }

  // Unknown → full command string (fallback)
  return `bash:${command}`;
}

/**
 * Compute the fingerprint for an edit / write / read tool call.
 * Currently falls back to toolName; path-based fingerprinting can be added later.
 */
export function computePathFingerprint(toolName: string, _args: Record<string, unknown>): string {
  return toolName;
}

/**
 * Compute the fingerprint for any tool call.
 * Dispatches to the appropriate strategy based on toolName.
 */
export function computeFingerprint(toolName: string, toolArgs: Record<string, unknown>): string {
  switch (toolName) {
    case "bash":
      return computeBashFingerprint(toolArgs);
    case "edit":
    case "write":
    case "read":
      return computePathFingerprint(toolName, toolArgs);
    default:
      return toolName;
  }
}

// ============================================================================
// ApprovalStore
// ============================================================================

export class ApprovalStore {
  private sessionApprovals = new Map<string, StoredApproval>(); // fingerprint → entry
  private workspacePath: string;
  private permanentPath: string;

  constructor(cwd: string) {
    const homeDir = process.env.HOME ?? process.env.USERPROFILE ?? ".";
    this.workspacePath = joinPath(cwd, ".piko", "approvals.json");
    this.permanentPath = joinPath(homeDir, ".piko", "approvals.json");
  }

  // ==========================================================================
  // Check
  // ==========================================================================

  /**
   * Check whether a tool call is already approved at any scope.
   * Computes the fingerprint for the given tool + args and checks all three
   * scopes. Returns the highest-priority scope (session > workspace > permanent),
   * or null if not approved.
   */
  isApproved(toolName: string, toolArgs: Record<string, unknown>): ApprovalScope | null {
    const fp = computeFingerprint(toolName, toolArgs);
    if (this.sessionApprovals.has(fp)) return "session";
    if (this.readApprovalsFile(this.workspacePath).fingerprints[fp]) return "workspace";
    if (this.readApprovalsFile(this.permanentPath).fingerprints[fp]) return "permanent";
    return null;
  }

  // ==========================================================================
  // Grant
  // ==========================================================================

  /**
   * Grant approval for a tool call at the given scope.
   */
  grant(toolName: string, toolArgs: Record<string, unknown>, scope: ApprovalScope): void {
    const now = Date.now();
    const fp = computeFingerprint(toolName, toolArgs);
    const entry: StoredApproval = {
      toolName,
      fingerprint: fp,
      sampleArgs: toolArgs,
      grantedAt: now,
    };
    switch (scope) {
      case "session":
        this.sessionApprovals.set(fp, entry);
        break;
      case "workspace":
        this.writeApproval(this.workspacePath, entry);
        break;
      case "permanent":
        this.writeApproval(this.permanentPath, entry);
        break;
    }
  }

  // ==========================================================================
  // Revoke
  // ==========================================================================

  /**
   * Revoke a specific fingerprint at the given scope.
   */
  revoke(fingerprint: string, scope: ApprovalScope): void {
    switch (scope) {
      case "session":
        this.sessionApprovals.delete(fingerprint);
        break;
      case "workspace":
        this.removeApproval(this.workspacePath, fingerprint);
        break;
      case "permanent":
        this.removeApproval(this.permanentPath, fingerprint);
        break;
    }
  }

  /**
   * Clear all session-level approvals (e.g., on session reset).
   */
  clearSession(): void {
    this.sessionApprovals.clear();
  }

  // ==========================================================================
  // Internals
  // ==========================================================================

  private readApprovalsFile(path: string): ApprovalsFile {
    try {
      if (!existsSync(path)) return { fingerprints: {} };
      const raw = readFileSync(path, "utf-8");
      const data = JSON.parse(raw);
      // Migrate old format: tools[toolName] → fingerprints[fingerprint]
      if (data.tools && !data.fingerprints) {
        return { fingerprints: {} };
      }
      return data as ApprovalsFile;
    } catch {
      return { fingerprints: {} };
    }
  }

  private writeApprovalsFile(path: string, data: ApprovalsFile): void {
    try {
      const dir = path.substring(0, path.lastIndexOf("/"));
      if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
      writeFileSync(path, JSON.stringify(data, null, 2), "utf-8");
    } catch {
      // Silently fail — approval persistence is best-effort
    }
  }

  private writeApproval(path: string, entry: StoredApproval): void {
    const data = this.readApprovalsFile(path);
    data.fingerprints[entry.fingerprint] = entry;
    this.writeApprovalsFile(path, data);
  }

  private removeApproval(path: string, fingerprint: string): void {
    const data = this.readApprovalsFile(path);
    delete data.fingerprints[fingerprint];
    this.writeApprovalsFile(path, data);
  }
}
