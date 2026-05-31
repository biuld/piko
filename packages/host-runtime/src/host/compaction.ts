import type { Model } from "@earendil-works/pi-ai";
import type { HostConfig } from "../models/index.js";
import type { SettingsManager } from "../settings/index.js";
import type { SessionManager } from "../session/index.js";
import {
  compact,
  estimateContextTokens,
  generateBranchSummary,
  prepareCompaction,
  shouldCompact,
  type CompactionSettings,
} from "../compaction/index.js";

export function getEffectiveCompactionSettings(
  settingsManager?: SettingsManager,
): CompactionSettings {
  if (settingsManager) {
    const s = settingsManager.getCompactionSettings();
    return { enabled: s.enabled, reserveTokens: s.reserveTokens, keepRecentTokens: s.keepRecentTokens };
  }
  return { enabled: true, reserveTokens: 16384, keepRecentTokens: 20000 };
}

export async function runCompact(
  sessionManager: SessionManager,
  config: HostConfig,
  settingsManager?: SettingsManager,
  _customInstructions?: string,
): Promise<void> {
  const s = getEffectiveCompactionSettings(settingsManager);
  const entries = await sessionManager.getBranch();
  const prep = prepareCompaction(entries, s);
  if (!prep.ok || !prep.value) return;
  const apiKey = config.provider.apiKey ?? "";
  const cr = await compact(prep.value, config.model as any, apiKey);
  if (!cr.ok) return;
  await sessionManager.appendCompaction(
    cr.value.summary, cr.value.firstKeptEntryId, cr.value.tokensBefore, cr.value.details,
  );
}

export async function runMaybeCompact(
  sessionManager: SessionManager,
  config: HostConfig,
  settingsManager?: SettingsManager,
): Promise<void> {
  const s = getEffectiveCompactionSettings(settingsManager);
  if (!s.enabled) return;
  const msgs = await sessionManager.loadMessages();
  const ctxTokens = estimateContextTokens(msgs as any).tokens;
  const cw = (config.model as { contextWindow?: number }).contextWindow ?? 200_000;
  if (shouldCompact(ctxTokens, cw, s)) await runCompact(sessionManager, config, settingsManager);
}

export async function generateAutoBranchSummary(
  sessionManager: SessionManager,
  config: HostConfig,
  settingsManager?: SettingsManager,
): Promise<string | undefined> {
  const bsSettings = settingsManager?.getBranchSummarySettings?.() ?? { reserveTokens: 16384, skipPrompt: false };
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
  } catch { /* non-fatal */ }
  return undefined;
}
