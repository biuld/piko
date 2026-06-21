import { EventStream, type ModelStepEvent } from "piko-orchestrator";
import type { PromptTemplate } from "../../prompts/index.js";
import type { Skill } from "../../skills/index.js";
import type { HostRuntimeConfigController } from "../runtime-config/index.js";
import type { HostRunResult, StreamPromptResult } from "../shared/index.js";
import { buildSkillPrompt, buildTemplatePrompt } from "./skills.js";

export class HostResourcesController {
  private readonly skillsValue: Skill[];
  private readonly promptTemplatesValue: PromptTemplate[];

  constructor(options: {
    skills: Skill[];
    promptTemplates?: PromptTemplate[];
    runtimeConfig: HostRuntimeConfigController;
    run: (prompt: string, signal?: AbortSignal) => Promise<HostRunResult>;
    streamPrompt: (
      prompt: string,
      signal?: AbortSignal,
    ) => EventStream<ModelStepEvent, StreamPromptResult>;
  }) {
    this.runtimeConfig = options.runtimeConfig;
    this.runPrompt = options.run;
    this.streamPromptText = options.streamPrompt;
    this.skillsValue = options.skills;
    this.promptTemplatesValue = options.promptTemplates ?? [];
  }

  private readonly runtimeConfig: HostRuntimeConfigController;
  private readonly runPrompt: (prompt: string, signal?: AbortSignal) => Promise<HostRunResult>;
  private readonly streamPromptText: (
    prompt: string,
    signal?: AbortSignal,
  ) => EventStream<ModelStepEvent, StreamPromptResult>;

  get skills(): Skill[] {
    return this.skillsValue;
  }

  get promptTemplates(): PromptTemplate[] {
    return this.promptTemplatesValue;
  }

  async runSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    const snapshot = this.applySkillOverrides(name);
    try {
      const prompt = buildSkillPrompt(this.skillsValue, name, additionalInstructions);
      return await this.runPrompt(prompt, signal);
    } finally {
      this.runtimeConfig.restoreSnapshot(snapshot);
    }
  }

  streamSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): EventStream<ModelStepEvent, StreamPromptResult> {
    try {
      const snapshot = this.applySkillOverrides(name);
      const prompt = buildSkillPrompt(this.skillsValue, name, additionalInstructions);
      const resultStream = this.streamPromptText(prompt, signal);

      const originalEnd = resultStream.end.bind(resultStream);
      resultStream.end = (value: StreamPromptResult) => {
        this.runtimeConfig.restoreSnapshot(snapshot);
        return originalEnd(value);
      };
      return resultStream;
    } catch (e: unknown) {
      const s = new EventStream<ModelStepEvent, StreamPromptResult>();
      s.push({ type: "error", message: e instanceof Error ? e.message : String(e) });
      s.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return s;
    }
  }

  async runPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    const prompt = buildTemplatePrompt(this.promptTemplatesValue, name, args);
    return this.runPrompt(prompt, signal);
  }

  streamPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): EventStream<ModelStepEvent, StreamPromptResult> {
    try {
      const prompt = buildTemplatePrompt(this.promptTemplatesValue, name, args);
      return this.streamPromptText(prompt, signal);
    } catch (e: unknown) {
      const s = new EventStream<ModelStepEvent, StreamPromptResult>();
      s.push({ type: "error", message: e instanceof Error ? e.message : String(e) });
      s.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return s;
    }
  }

  private applySkillOverrides(name: string) {
    const skill = this.skillsValue.find((s) => s.name === name);
    if (!skill) throw new Error(`Unknown skill: ${name}`);

    const snapshot = this.runtimeConfig.snapshot();
    const currentConfig = this.runtimeConfig.getConfig();
    if (skill.modelOverride) {
      const [provider, modelId] = skill.modelOverride.split("/");
      if (provider && modelId) {
        this.runtimeConfig.applyTemporary({
          config: {
            ...currentConfig,
            model: { ...currentConfig.model, provider, id: modelId },
          },
        });
      }
    }
    if (skill.thinkingLevel) {
      this.runtimeConfig.applyTemporary({ thinkingLevel: skill.thinkingLevel });
    }
    if (skill.activeTools !== undefined) {
      this.runtimeConfig.applyTemporary({
        activeToolNames: skill.activeTools
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      });
    }
    return snapshot;
  }
}
