/**
 * Antigravity (Google Cloud Code) model definitions.
 *
 * Registered as custom provider models in piko's ModelRegistry.
 * Mirrors the models from @raquezha/noagy.
 */

import type { Api, Model } from "../../orchd/protocol/index.js";

const ANTIGRAVITY_MODEL_VARIANTS: Array<{
  id: string;
  name: string;
  reasoning: boolean;
  input: Array<"text" | "image">;
  contextWindow: number;
  maxTokens: number;
}> = [
  {
    id: "gemini-3.5-flash-low",
    name: "Gemini 3.5 Flash Low (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.5-flash",
    name: "Gemini 3.5 Flash Medium (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.5-flash-high",
    name: "Gemini 3.5 Flash High (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.1-pro-low",
    name: "Gemini 3.1 Pro Low (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.1-pro-high",
    name: "Gemini 3.1 Pro High (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "claude-sonnet-4-6-thinking",
    name: "Claude Sonnet 4.6 Thinking (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 200000,
    maxTokens: 64000,
  },
  {
    id: "claude-opus-4-6-thinking",
    name: "Claude Opus 4.6 Thinking (Antigravity)",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 200000,
    maxTokens: 128000,
  },
  {
    id: "gpt-oss-120b-medium",
    name: "GPT-OSS 120B Medium (Antigravity)",
    reasoning: false,
    input: ["text"],
    contextWindow: 131072,
    maxTokens: 32768,
  },
];

export function createAntigravityModels(): Model<string>[] {
  return ANTIGRAVITY_MODEL_VARIANTS.map((variant) => ({
    id: variant.id,
    name: variant.name,
    api: "antigravity-api" as Api,
    provider: "antigravity",
    baseUrl: "https://daily-cloudcode-pa.googleapis.com",
    reasoning: variant.reasoning,
    thinkingLevelMap: variant.reasoning ? ({ xhigh: "HIGH" } as any) : undefined,
    input: variant.input,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: variant.contextWindow,
    maxTokens: variant.maxTokens,
    headers: undefined,
  })) as Model<string>[];
}
