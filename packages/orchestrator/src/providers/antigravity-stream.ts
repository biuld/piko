/**
 * Antigravity (Google Cloud Code) stream handler.
 *
 * Reimplements @raquezha/noagy stream logic as a built-in piko provider.
 * Converts pi messages to Google Antigravity format, calls the streaming
 * API, and translates SSE events back to pi AssistantMessageEventStream.
 */

import {
  type AssistantMessageEventStream,
  type Context,
  createAssistantMessageEventStream,
  type Model,
} from "@earendil-works/pi-ai";

// ============================================================================
// Constants
// ============================================================================

const PROVIDER_ID = "antigravity";
const DEFAULT_RUNTIME_MODEL_ID = "gemini-3.5-flash-low";
const DEFAULT_ENDPOINT = "https://daily-cloudcode-pa.googleapis.com";
const ENDPOINT_FALLBACKS = [DEFAULT_ENDPOINT];

const MODEL_VARIANTS: Array<{
  id: string;
  runtimeModel: string;
  reasoning: boolean;
  input: Array<"text" | "image">;
  contextWindow: number;
  maxTokens: number;
}> = [
  {
    id: "gemini-3.5-flash-low",
    runtimeModel: "gemini-3.5-flash-extra-low",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.5-flash",
    runtimeModel: "gemini-3.5-flash-low",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.5-flash-high",
    runtimeModel: "gemini-3-flash-agent",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.1-pro-low",
    runtimeModel: "gemini-3.1-pro-low",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "gemini-3.1-pro-high",
    runtimeModel: "gemini-pro-agent",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 1048576,
    maxTokens: 65535,
  },
  {
    id: "claude-sonnet-4-6-thinking",
    runtimeModel: "claude-sonnet-4-6",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 200000,
    maxTokens: 64000,
  },
  {
    id: "claude-opus-4-6-thinking",
    runtimeModel: "claude-opus-4-6-thinking",
    reasoning: true,
    input: ["text", "image"],
    contextWindow: 200000,
    maxTokens: 128000,
  },
  {
    id: "gpt-oss-120b-medium",
    runtimeModel: "gpt-oss-120b-medium",
    reasoning: false,
    input: ["text"],
    contextWindow: 131072,
    maxTokens: 32768,
  },
];

const ANTIGRAVITY_SYSTEM_INSTRUCTION =
  "You are Antigravity, a powerful agentic AI coding assistant designed by Google DeepMind. " +
  "You are pair programming with a user to solve coding tasks. Be concise, practical, and tool-aware.";

// ============================================================================
// Helpers
// ============================================================================

function antigravityEnv(name: string): string | undefined {
  return process.env[`ANTIGRAVITY_${name}`] || process.env[`NOAGY_${name}`];
}

function endpointCandidates(): string[] {
  const explicit = antigravityEnv("BASE_URL")?.trim();
  return explicit ? [explicit] : ENDPOINT_FALLBACKS;
}

function nowRequestId(): string {
  return `antigravity-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
}

function sanitizeText(text: unknown): string {
  return String(text ?? "").replace(/[\uD800-\uDFFF]/g, "\uFFFD");
}

function parseApiKey(apiKeyRaw: string | undefined): { token: string; projectId: string } {
  if (!apiKeyRaw) throw new Error("No Antigravity OAuth credentials. Run /login antigravity.");
  try {
    const parsed = JSON.parse(apiKeyRaw) as { token?: string; projectId?: string };
    if (!parsed.token || !parsed.projectId) throw new Error("missing token or projectId");
    return { token: parsed.token, projectId: parsed.projectId };
  } catch (error) {
    throw new Error(
      `Invalid Antigravity credentials. Run /login antigravity. (${error instanceof Error ? error.message : String(error)})`,
    );
  }
}

function antigravityHeaders(token: string): Record<string, string> {
  return {
    Authorization: `Bearer ${token}`,
    "Content-Type": "application/json",
    Accept: "text/event-stream",
    "User-Agent": antigravityEnv("USER_AGENT") || "antigravity/1.0.5 darwin/arm64",
    "X-Goog-Api-Client": "google-api-nodejs-client/9.15.1",
    "Client-Metadata": JSON.stringify({ ideType: "ANTIGRAVITY" }),
  };
}

// ============================================================================
// Runtime model resolution
// ============================================================================

function runtimeModelFor(modelId: string): string {
  return MODEL_VARIANTS.find((v) => v.id === modelId)?.runtimeModel || DEFAULT_RUNTIME_MODEL_ID;
}

// ============================================================================
// Project discovery (warm-up)
// ============================================================================

function extractProjectId(data: unknown): string | undefined {
  if (!data || typeof data !== "object") return undefined;
  const obj = data as Record<string, unknown>;
  const direct =
    obj.antigravityProjectId ??
    obj.projectId ??
    obj.backendProjectId ??
    obj.userDefinedCloudaicompanionProject ??
    obj.cloudaicompanionProject ??
    obj.project;
  if (typeof direct === "string" && direct) return direct;
  if (
    direct &&
    typeof direct === "object" &&
    typeof (direct as Record<string, unknown>).id === "string"
  ) {
    return (direct as Record<string, unknown>).id as string;
  }
  for (const key of ["projects", "projectIds", "cloudaicompanionProjects"]) {
    const value = obj[key];
    if (Array.isArray(value)) {
      for (const item of value) {
        const nested = extractProjectId(item);
        if (nested) return nested;
        if (typeof item === "string" && item) return item;
      }
    }
  }
  return undefined;
}

async function loadCodeAssist(token: string): Promise<string | undefined> {
  const body = JSON.stringify({ metadata: { ideType: "ANTIGRAVITY" } });
  for (const endpoint of endpointCandidates()) {
    try {
      const res = await fetch(`${endpoint}/v1internal:loadCodeAssist`, {
        method: "POST",
        headers: antigravityHeaders(token),
        body,
      });
      if (!res.ok) continue;
      const project = extractProjectId(await res.json());
      if (project) return project;
    } catch {
      // continue
    }
  }
  return undefined;
}

// ============================================================================
// Message conversion
// ============================================================================

function asTextParts(content: unknown): any[] {
  if (typeof content === "string") return [{ text: sanitizeText(content) }];
  if (!Array.isArray(content)) return [];
  return content.flatMap<any>((item) => {
    if (!item || typeof item !== "object") return [];
    const block = item as any;
    if (block.type === "text") return [{ text: sanitizeText(block.text) }];
    if (block.type === "image") {
      const data = block.data || block.source?.data;
      const mimeType = block.mimeType || block.mediaType || block.source?.mediaType || "image/png";
      return data ? [{ inlineData: { mimeType, data } }] : [];
    }
    return [];
  });
}

function toolCallIdNeeded(modelId: string): boolean {
  return modelId.startsWith("claude-") || modelId.startsWith("gpt-oss-");
}

function convertMessages(model: any, context: Context): any[] {
  const contents: any[] = [];
  const messages = Array.isArray(context.messages) ? context.messages : [];

  for (const msg of messages) {
    if (msg.role === "user") {
      const parts = asTextParts(msg.content);
      if (parts.length) contents.push({ role: "user", parts });
    } else if (msg.role === "assistant") {
      const parts: any[] = [];
      for (const block of msg.content || []) {
        if (block.type === "text" && String(block.text || "").trim()) {
          parts.push({ text: sanitizeText(block.text) });
        } else if (block.type === "thinking" && String(block.thinking || "").trim()) {
          if (msg.provider === PROVIDER_ID && msg.model === model.id) {
            parts.push({
              thought: true,
              text: sanitizeText(block.thinking),
              ...(block.thinkingSignature ? { thoughtSignature: block.thinkingSignature } : {}),
            });
          } else {
            parts.push({ text: sanitizeText(block.thinking) });
          }
        } else if (block.type === "toolCall") {
          parts.push({
            functionCall: {
              name: block.name,
              args: block.arguments ?? {},
              ...(toolCallIdNeeded(model.id) ? { id: block.id } : {}),
            },
            ...(block.thoughtSignature ? { thoughtSignature: block.thoughtSignature } : {}),
          });
        }
      }
      if (parts.length) contents.push({ role: "model", parts });
    } else if (msg.role === "toolResult") {
      const content = Array.isArray(msg.content) ? msg.content : [];
      const text = content
        .filter((c: any) => c.type === "text")
        .map((c: any) => sanitizeText(c.text))
        .join("\n");
      const responseText = text || (msg.isError ? "Tool failed" : "");
      const part = {
        functionResponse: {
          name: msg.toolName,
          response: msg.isError ? { error: responseText } : { output: responseText },
          ...(toolCallIdNeeded(model.id) ? { id: msg.toolCallId } : {}),
        },
      };
      const last = contents[contents.length - 1];
      if (last?.role === "user" && last.parts?.some((p: any) => p.functionResponse)) {
        last.parts.push(part);
      } else {
        contents.push({ role: "user", parts: [part] });
      }
    }
  }
  return contents;
}

// ============================================================================
// Tool conversion
// ============================================================================

function stripMetaSchema(schema: unknown): unknown {
  if (!schema || typeof schema !== "object" || Array.isArray(schema)) return schema;
  const omit = new Set([
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$vocabulary",
    "$comment",
    "$defs",
    "definitions",
  ]);
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(schema)) {
    if (!omit.has(key)) out[key] = stripMetaSchema(value);
  }
  return out;
}

function normalizeGoogleSchema(schema: unknown): unknown {
  if (!schema || typeof schema !== "object") return schema;
  if (Array.isArray(schema)) return schema.map(normalizeGoogleSchema);
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(schema)) {
    if (key === "type" && typeof value === "string") out[key] = value.toUpperCase();
    else out[key] = normalizeGoogleSchema(value);
  }
  return out;
}

function convertTools(tools: any[] | undefined, useLegacyParameters = false): any[] | undefined {
  if (!tools?.length) return undefined;
  return [
    {
      functionDeclarations: tools.map((tool) => {
        const parameters = stripMetaSchema(tool.parameters);
        return {
          name: tool.name,
          description: tool.description,
          ...(useLegacyParameters
            ? { parameters: normalizeGoogleSchema(parameters) }
            : { parametersJsonSchema: parameters }),
        };
      }),
    },
  ];
}

// ============================================================================
// Request building
// ============================================================================

function thinkingLevel(reasoning: string | undefined): string | undefined {
  switch (reasoning) {
    case "minimal":
      return "MINIMAL";
    case "low":
      return "LOW";
    case "medium":
      return "MEDIUM";
    case "high":
    case "xhigh":
      return "HIGH";
    default:
      return undefined;
  }
}

interface StreamOptions {
  apiKey?: string;
  baseUrl?: string;
  headers?: Record<string, string>;
  reasoning?: string;
  signal?: AbortSignal;
  sessionId?: string;
  temperature?: number;
  maxTokens?: number;
  toolChoice?: string;
}

function buildRequest(
  model: any,
  context: Context,
  projectId: string,
  options: StreamOptions,
  runtimeModel: string,
): any {
  const request: any = {
    contents: convertMessages(model, context),
    systemInstruction: {
      role: "user",
      parts: [
        { text: ANTIGRAVITY_SYSTEM_INSTRUCTION },
        ...(context.systemPrompt ? [{ text: sanitizeText(context.systemPrompt) }] : []),
      ],
    },
  };

  const generationConfig: any = {};
  if (options.temperature !== undefined) generationConfig.temperature = options.temperature;
  if (options.maxTokens !== undefined) generationConfig.maxOutputTokens = options.maxTokens;
  else generationConfig.maxOutputTokens = Math.min(8192, model.maxTokens || 8192);

  const level = model.reasoning ? thinkingLevel(options.reasoning) : undefined;
  if (level) generationConfig.thinkingConfig = { includeThoughts: true, thinkingLevel: level };
  if (Object.keys(generationConfig).length) request.generationConfig = generationConfig;

  const tools = convertTools(context.tools, runtimeModel.startsWith("claude-"));
  if (tools) request.tools = tools;

  if (options.toolChoice) {
    request.toolConfig = {
      functionCallingConfig: {
        mode:
          options.toolChoice === "none" ? "NONE" : options.toolChoice === "any" ? "ANY" : "AUTO",
      },
    };
  }

  if (options.sessionId) request.sessionId = options.sessionId;

  return {
    project: projectId,
    model: runtimeModel,
    request,
    requestType: "agent",
    userAgent: "antigravity",
    requestId: nowRequestId(),
  };
}

// ============================================================================
// Stream response parsing
// ============================================================================

function createOutput(model: any): any {
  return {
    role: "assistant",
    content: [],
    api: "antigravity-api",
    provider: PROVIDER_ID,
    model: model.id,
    usage: {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    },
    stopReason: "stop",
    timestamp: Date.now(),
  };
}

async function streamResponse(
  response: Response,
  stream: AssistantMessageEventStream,
  output: any,
): Promise<boolean> {
  if (!response.body) throw new Error("No response body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let started = false;
  let currentBlock: any = null;
  let hasContent = false;
  const blocks = output.content;
  const blockIndex = () => blocks.length - 1;

  const ensureStarted = () => {
    if (!started) {
      stream.push({ type: "start", partial: output });
      started = true;
    }
  };

  const finishCurrent = () => {
    if (!currentBlock) return;
    if (currentBlock.type === "text")
      stream.push({
        type: "text_end",
        contentIndex: blockIndex(),
        content: currentBlock.text,
        partial: output,
      });
    else if (currentBlock.type === "thinking")
      stream.push({
        type: "thinking_end",
        contentIndex: blockIndex(),
        content: currentBlock.thinking,
        partial: output,
      });
    currentBlock = null;
  };

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() || "";

    for (const line of lines) {
      if (!line.startsWith("data:")) continue;
      const json = line.slice(5).trim();
      if (!json || json === "[DONE]") continue;

      let chunk: any;
      try {
        chunk = JSON.parse(json);
      } catch {
        continue;
      }

      if (chunk.error) throw new Error(chunk.error.message || JSON.stringify(chunk.error));

      const responseData = chunk.response || chunk;
      const candidate = responseData.candidates?.[0];

      for (const part of candidate?.content?.parts || []) {
        if (part.text !== undefined) {
          hasContent = true;
          const isThinking = part.thought === true;
          const type = isThinking ? "thinking" : "text";

          if (!currentBlock || currentBlock.type !== type) {
            finishCurrent();
            currentBlock = isThinking
              ? { type: "thinking", thinking: "", thinkingSignature: undefined }
              : { type: "text", text: "" };
            blocks.push(currentBlock);
            ensureStarted();
            stream.push({
              type: isThinking ? "thinking_start" : "text_start",
              contentIndex: blockIndex(),
              partial: output,
            });
          }

          if (isThinking) {
            currentBlock.thinking += part.text;
            if (part.thoughtSignature) currentBlock.thinkingSignature = part.thoughtSignature;
            stream.push({
              type: "thinking_delta",
              contentIndex: blockIndex(),
              delta: part.text,
              partial: output,
            });
          } else {
            currentBlock.text += part.text;
            if (part.thoughtSignature) currentBlock.textSignature = part.thoughtSignature;
            stream.push({
              type: "text_delta",
              contentIndex: blockIndex(),
              delta: part.text,
              partial: output,
            });
          }
        }

        if (part.functionCall) {
          hasContent = true;
          finishCurrent();
          const toolCall = {
            type: "toolCall" as const,
            id:
              part.functionCall.id ||
              `${part.functionCall.name || "tool"}_${Date.now()}_${blocks.length}`,
            name: part.functionCall.name || "",
            arguments: part.functionCall.args || {},
            ...(part.thoughtSignature ? { thoughtSignature: part.thoughtSignature } : {}),
          };
          blocks.push(toolCall);
          ensureStarted();
          stream.push({
            type: "toolcall_start",
            contentIndex: blockIndex(),
            partial: output,
          });
          stream.push({
            type: "toolcall_delta",
            contentIndex: blockIndex(),
            delta: JSON.stringify(toolCall.arguments),
            partial: output,
          });
          stream.push({
            type: "toolcall_end",
            contentIndex: blockIndex(),
            toolCall,
            partial: output,
          });
        }
      }

      if (candidate?.finishReason) {
        output.stopReason = blocks.some((b: any) => b.type === "toolCall")
          ? "toolUse"
          : candidate.finishReason === "STOP"
            ? "stop"
            : candidate.finishReason === "MAX_TOKENS"
              ? "length"
              : "error";
      }

      if (responseData.usageMetadata) {
        const prompt = responseData.usageMetadata.promptTokenCount || 0;
        const cacheRead = responseData.usageMetadata.cachedContentTokenCount || 0;
        output.usage.input = prompt - cacheRead;
        output.usage.output =
          (responseData.usageMetadata.candidatesTokenCount || 0) +
          (responseData.usageMetadata.thoughtsTokenCount || 0);
        output.usage.cacheRead = cacheRead;
        output.usage.totalTokens = responseData.usageMetadata.totalTokenCount || 0;
      }
    }
  }

  finishCurrent();
  return hasContent;
}

// ============================================================================
// Main stream function
// ============================================================================

function streamNoagy(
  model: Model<string>,
  context: Context,
  options?: StreamOptions,
): AssistantMessageEventStream {
  const stream = createAssistantMessageEventStream();

  void (async () => {
    const output = createOutput(model);
    try {
      const creds = parseApiKey(options?.apiKey);
      const warmedProject = await loadCodeAssist(creds.token);
      const projectId = antigravityEnv("PROJECT_ID")?.trim() || warmedProject || creds.projectId;
      const runtimeModel = antigravityEnv("RUNTIME_MODEL")?.trim() || runtimeModelFor(model.id);

      const body = JSON.stringify(
        buildRequest(model, context, projectId, options || {}, runtimeModel),
      );

      let response: Response | undefined;
      let lastText = "";
      let lastEndpoint: string | undefined;

      for (const endpoint of endpointCandidates()) {
        lastEndpoint = endpoint;
        response = await fetch(`${endpoint}/v1internal:streamGenerateContent?alt=sse`, {
          method: "POST",
          headers: antigravityHeaders(creds.token),
          body,
          signal: options?.signal,
        });
        if (response.ok) break;
        lastText = await response.text();
        if (![403, 404, 429, 500, 502, 503, 504].includes(response.status)) break;
      }

      if (!response?.ok) {
        throw new Error(
          `Antigravity API error (${response?.status ?? "no response"}, endpoint=${lastEndpoint || "unknown"}): ${lastText}`,
        );
      }

      const received = await streamResponse(response, stream, output);
      if (!received) throw new Error("Antigravity API returned an empty response");

      stream.push({ type: "done", reason: output.stopReason, message: output });
      stream.end();
    } catch (error) {
      output.stopReason = options?.signal?.aborted ? "aborted" : "error";
      output.errorMessage = error instanceof Error ? error.message : String(error);
      stream.push({ type: "error", reason: output.stopReason, error: output });
      stream.end();
    }
  })();

  return stream;
}

export { streamNoagy };
