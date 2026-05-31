import {
  type CompactionSettings,
  compact,
  estimateContextTokens,
  generateBranchSummary,
  prepareCompaction,
  shouldCompact,
} from "../compaction/index.js";
import type { HostConfig } from "../models/index.js";
import type { SessionManager } from "../session/index.js";
import type { SettingsManager } from "../settings/index.js";

export function getEffectiveCompactionSettings(
  settingsManager?: SettingsManager,
): CompactionSettings {
  if (settingsManager) {
    const s = settingsManager.getCompactionSettings();
    return {
      enabled: s.enabled,
      reserveTokens: s.reserveTokens,
      keepRecentTokens: s.keepRecentTokens,
    };
  }
  return { enabled: true, reserveTokens: 16384, keepRecentTokens: 20000 };
}

/** Result of a compaction run. */
export interface CompactResult {
  /** Whether compaction was performed. */
  compacted: boolean;
  /** Reason compaction was skipped, when applicable. */
  skippedReason?: string;
  /** Error that occurred during compaction. */
  error?: string;
  /** Tokens before compaction (when performed). */
  tokensBefore?: number;
  /** Tokens kept after compaction (when performed). */
  tokensKept?: number;
}

export async function runCompact(
  sessionManager: SessionManager,
  config: HostConfig,
  settingsManager?: SettingsManager,
  customInstructions?: string,
): Promise<CompactResult> {
  const s = getEffectiveCompactionSettings(settingsManager);
  if (!s.enabled) {
    return { compacted: false, skippedReason: "compaction disabled" };
  }

  const entries = await sessionManager.getBranch();
  const prep = prepareCompaction(entries, s);
  if (!prep.ok) {
    return { compacted: false, error: `prepare failed: ${prep.error.message}` };
  }
  if (!prep.value) {
    return { compacted: false, skippedReason: "nothing to compact" };
  }

  const apiKey = config.provider.apiKey ?? "";
  if (!apiKey) {
    return { compacted: false, error: "no API key configured" };
  }

  const cr = await compact(prep.value, config.model as any, apiKey, undefined, customInstructions);
  if (!cr.ok) {
    return { compacted: false, error: `summarization failed: ${cr.error.message}` };
  }

  await sessionManager.appendCompaction(
    cr.value.summary,
    cr.value.firstKeptEntryId,
    cr.value.tokensBefore,
    cr.value.details,
  );

  return {
    compacted: true,
    tokensBefore: cr.value.tokensBefore,
    tokensKept: s.keepRecentTokens,
  };
}

export async function runMaybeCompact(
  sessionManager: SessionManager,
  config: HostConfig,
  settingsManager?: SettingsManager,
): Promise<CompactResult> {
  const s = getEffectiveCompactionSettings(settingsManager);
  if (!s.enabled) return { compacted: false, skippedReason: "compaction disabled" };

  const msgs = await sessionManager.loadMessages();
  const ctxTokens = estimateContextTokens(msgs as any).tokens;
  const cw = (config.model as { contextWindow?: number }).contextWindow ?? 200_000;
  if (!shouldCompact(ctxTokens, cw, s)) {
    return { compacted: false, skippedReason: `below threshold (${ctxTokens}/${cw})` };
  }
  return runCompact(sessionManager, config, settingsManager);
}

export async function generateAutoBranchSummary(
  sessionManager: SessionManager,
  config: HostConfig,
  settingsManager?: SettingsManager,
): Promise<string | undefined> {
  const bsSettings = settingsManager?.getBranchSummarySettings?.() ?? {
    reserveTokens: 16384,
    skipPrompt: false,
  };
  if (bsSettings.skipPrompt) return undefined;

  try {
    const entries = await sessionManager.getBranch();
    if (entries.length === 0) return undefined;

    const apiKey = config.provider.apiKey ?? "";
    if (!apiKey) return undefined;

    const result = await generateBranchSummary(entries, {
      model: config.model as any,
      apiKey,
      signal: new AbortController().signal,
      reserveTokens: bsSettings.reserveTokens,
    });

    if (result.ok) {
      const msg = result.value.summary;
      if (msg && msg !== "No content to summarize") return msg;
    }
  } catch {
    /* non-fatal */
  }
  return undefined;
}
