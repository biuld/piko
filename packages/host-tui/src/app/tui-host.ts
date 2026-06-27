import type {
  ContextFile,
  FlatTreeEntry,
  Message,
  OrchState,
  SessionTreeEntry,
  TreeNavigationResult,
} from "../shared/index.js";
import type { TuiHostConfig as HostConfig } from "./host-config.js";

export interface TuiHostFacade {
  cwd: string;
  sessionId: string;
  sessionFile: string;
  teamMode: boolean;
  version: string;
  debugTracePath?: string;
  getConfig(): HostConfig;
  setConfig(config: HostConfig): void;
  getThinkingLevel(): string | undefined;
  setThinkingLevel(level: string): void;
  setLifecycleCallback(callback: (event: unknown) => void): void;
  restoreFromSession(): Promise<void>;
  loadMessages(): Promise<Message[]>;
  loadBranchEntries(): Promise<SessionTreeEntry[]>;
  getSessionName(): Promise<string | null>;
  setSessionName(name?: string): Promise<void>;
  newSession(): Promise<void>;
  cloneSession(name?: string): Promise<void>;
  switchSession(sessionId: string, entryId?: string): Promise<unknown>;
  navigateToEntry(entryId: string, signal?: AbortSignal): Promise<TreeNavigationResult>;
  forkSession(entryId?: string): Promise<{ selectedText?: string }>;
  importSession(path: string): Promise<void>;
  renameSession(sessionId: string, name: string): Promise<void>;
  listSessions(...args: unknown[]): Promise<any[]>;
  getLeafId(): Promise<string | null | undefined>;
  getTreeEntries(): Promise<SessionTreeEntry[]>;
  getContextFiles(): ContextFile[];
  getActiveToolNames(): string[];
  getTotalToolCount(): number;
  getOrchestratorSnapshot(): OrchState | undefined;
  prompt(...args: unknown[]): any;
  dequeue(agentId?: string): {
    steering: Array<{ text: string }>;
    followUp: Array<{ text: string }>;
    nextTurn: Array<{ text: string }>;
  };
  runSkill(...args: unknown[]): Promise<void>;
  runPromptTemplate(...args: unknown[]): Promise<void>;
  compact(): Promise<any>;
  setSteeringMode(mode: string): void;
  setFollowUpMode(mode: string): void;
}

export type TuiHostConfig = HostConfig;
export type TuiOrchState = OrchState;
export type TuiContextFile = ContextFile;
export type TuiFlatTreeEntry = FlatTreeEntry;
