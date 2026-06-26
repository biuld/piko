import { err, ok, type Result } from "piko-session";
import type { Model, Orchestrator } from "../orchd/protocol/index.js";
import type { ThinkingLevel } from "../types.js";
import { generateSummary, generateTurnPrefixSummary } from "./summarization.js";
import type { CompactionDetails, CompactionPreparation, CompactionResult } from "./types.js";
import { CompactionError } from "./types.js";
import { computeFileLists, formatFileOperations } from "./utils.js";

/** Generate compaction summary data from prepared session history. */
export async function compact(
  preparation: CompactionPreparation,
  model: Model,
  orchestrator: Orchestrator,
  customInstructions?: string,
  signal?: AbortSignal,
  thinkingLevel?: ThinkingLevel,
): Promise<Result<CompactionResult, CompactionError>> {
  const {
    firstKeptEntryId,
    messagesToSummarize,
    turnPrefixMessages,
    isSplitTurn,
    tokensBefore,
    previousSummary,
    fileOps,
    settings,
  } = preparation;

  if (!firstKeptEntryId) {
    return err(
      new CompactionError(
        "invalid_session",
        "First kept entry has no UUID - session may need migration",
      ),
    );
  }

  let summary: string;

  if (isSplitTurn && turnPrefixMessages.length > 0) {
    const [historyResult, turnPrefixResult] = await Promise.all([
      messagesToSummarize.length > 0
        ? generateSummary(
            messagesToSummarize,
            model,
            settings.reserveTokens,
            orchestrator,
            signal,
            customInstructions,
            previousSummary,
            thinkingLevel,
          )
        : Promise.resolve(ok<string, CompactionError>("No prior history.")),
      generateTurnPrefixSummary(
        turnPrefixMessages,
        model,
        settings.reserveTokens,
        orchestrator,
        signal,
        thinkingLevel,
      ),
    ]);
    if (!historyResult.ok) return err(historyResult.error);
    if (!turnPrefixResult.ok) return err(turnPrefixResult.error);
    summary = `${historyResult.value}\n\n---\n\n**Turn Context (split turn):**\n\n${turnPrefixResult.value}`;
  } else {
    const summaryResult = await generateSummary(
      messagesToSummarize,
      model,
      settings.reserveTokens,
      orchestrator,
      signal,
      customInstructions,
      previousSummary,
      thinkingLevel,
    );
    if (!summaryResult.ok) return err(summaryResult.error);
    summary = summaryResult.value;
  }

  const { readFiles, modifiedFiles } = computeFileLists(fileOps);
  summary += formatFileOperations(readFiles, modifiedFiles);

  return ok({
    summary,
    firstKeptEntryId,
    tokensBefore,
    details: { readFiles, modifiedFiles } as CompactionDetails,
  });
}
