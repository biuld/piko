import type { CommandDefinition } from "../types.js";
import type { BuiltinCommandContext } from "./types.js";

export function createDoctorCommands(ctx: BuiltinCommandContext): CommandDefinition[] {
  return [
    {
      id: "piko.doctor",
      slash: {
        name: "/doctor",
        description: "Show diagnostic information for the current session",
      },
      run(_ctx) {
        const state = ctx().getState();
        const host = ctx().host;
        const currentModel = state.model.current;

        const lines = [
          `Model: ${currentModel.id} (${currentModel.provider})`,
          `Thinking Level: ${state.model.thinkingLevel}`,
          `Session ID: ${host.sessionId}`,
        ];

        if (host.debugTracePath) {
          lines.push(`Debug Trace: ${host.debugTracePath}`);
        } else {
          lines.push("Debug Trace: Disabled (run with PIKO_DEBUG=1 to enable)");
        }

        ctx().notify(`Diagnostic Info:\n${lines.join("\n")}`, "info");
      },
    },
  ];
}
