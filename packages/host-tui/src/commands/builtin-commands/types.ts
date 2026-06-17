import type { PikoHost } from "piko-host-runtime";

export interface BuiltinCommandDeps {
  openPanel: (request: any) => string;
  closeSurface: (id?: string) => void;
  notify: (message: string, severity?: string) => void;
  getState: () => any;
  executeCommand: (commandId: string, args?: string) => void;
  shutdown: () => void;
  abort: () => void;
  host: PikoHost;
  dispatch: (event: any) => void;
  switchModel: (modelId: string, provider: string) => boolean;
  modelRegistry?: any;
}

export type BuiltinCommandContext = () => BuiltinCommandDeps;
