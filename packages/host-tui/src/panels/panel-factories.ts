import type { PanelSession } from "./types.js";

let panelIdCounter = 0;
function nextId(prefix = "panel"): string {
  return `${prefix}-${++panelIdCounter}`;
}

export function createHelpPanelSession(): PanelSession {
  return {
    id: nextId("help"),
    stack: [
      {
        id: "help.list",
        chrome: {
          title: "Commands Help",
          hints: ["Up/Down move  Esc close"],
        },
        interaction: "list",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "help",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createSettingsPanelSession(): PanelSession {
  return {
    id: nextId("settings"),
    stack: [
      {
        id: "settings.root",
        chrome: {
          title: "Settings",
          hints: ["Up/Down move  Enter change  Esc close"],
        },
        interaction: "menu",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "settings",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createResumePanelSession(scope: "all" | "project" = "all"): PanelSession {
  return {
    id: nextId("resume"),
    stack: [
      {
        id: "resume.list",
        chrome: {
          title: "Resume Session",
          hints: ["Type filter  Up/Down move  Enter open  Esc close"],
        },
        interaction: "list",
        capabilities: [
          { kind: "filter", placeholder: "Filter sessions..." },
          { kind: "list", selectable: true },
        ],
        body: {
          type: "session-resume",
          payload: { scope },
        },
      },
    ],
    state: { filterText: "", selectedIndex: 0 },
  };
}

export function createToolApprovalPanelSession(): PanelSession {
  return {
    id: nextId("tool-approval"),
    stack: [
      {
        id: "tool-approval.main",
        chrome: {
          title: "Tool Approval",
          hints: ["Enter accept", "Esc decline"],
          height: 9,
        },
        interaction: "passive",
        capabilities: [],
        body: {
          type: "tool-approval",
          payload: {},
        },
      },
    ],
    state: {},
  };
}

export function createModelPickerPanelSession(): PanelSession {
  return {
    id: nextId("model-picker"),
    stack: [
      {
        id: "model.list",
        chrome: {
          title: "Models",
          hints: ["Type filter  Up/Down move  Enter select  Esc close"],
        },
        interaction: "list",
        capabilities: [
          { kind: "filter", placeholder: "Filter models..." },
          { kind: "list", selectable: true },
        ],
        body: {
          type: "model-picker",
          payload: {},
        },
      },
    ],
    state: { filterText: "", selectedIndex: 0 },
  };
}

export function createThinkingPanelSession(): PanelSession {
  return {
    id: nextId("thinking"),
    stack: [
      {
        id: "thinking.list",
        chrome: {
          title: "Thinking Level",
          hints: ["Up/Down move  Enter select  Esc close"],
        },
        interaction: "list",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "thinking-picker",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createLoginPanelSession(provider?: string): PanelSession {
  if (!provider) {
    return {
      id: nextId("login"),
      stack: [
        {
          id: "login.provider-picker",
          chrome: {
            title: "Select Provider",
            hints: ["Up/Down move  Enter select  Esc close"],
          },
          interaction: "list",
          capabilities: [{ kind: "list", selectable: true }],
          body: {
            type: "provider-picker",
            payload: {},
          },
        },
      ],
      state: { selectedIndex: 0 },
    };
  }

  return {
    id: nextId("login"),
    stack: [
      {
        id: "login.form",
        chrome: {
          title: `Login - ${provider}`,
          hints: ["Enter submit  Esc close"],
        },
        interaction: "form",
        capabilities: [],
        body: {
          type: "login",
          payload: { provider },
        },
      },
    ],
    state: {},
  };
}

export function createForkSessionPanelSession(): PanelSession {
  return {
    id: nextId("fork"),
    stack: [
      {
        id: "fork.list",
        chrome: {
          title: "Fork Session",
          hints: ["Up/Down move  Enter fork  Esc close"],
        },
        interaction: "menu",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "session-fork",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createTreePanelSession(): PanelSession {
  return {
    id: nextId("tree"),
    stack: [
      {
        id: "tree.list",
        chrome: {
          title: "Session Tree",
          hints: ["↑↓ move  Enter open  f mode  Esc close"],
        },
        interaction: "list",
        capabilities: [
          { kind: "filter", placeholder: "Filter tree..." },
          { kind: "list", selectable: true },
        ],
        body: {
          type: "session-tree",
          payload: {},
        },
      },
    ],
    state: { filterText: "", selectedIndex: 0 },
  };
}

export function createRenameSessionPanelSession(): PanelSession {
  return {
    id: nextId("rename"),
    stack: [
      {
        id: "rename.form",
        chrome: {
          title: "Rename Session",
          hints: ["Enter submit  Esc close"],
        },
        interaction: "form",
        capabilities: [],
        body: {
          type: "session-rename",
          payload: {},
        },
      },
    ],
    state: {},
  };
}

export function createNotificationsPanelSession(): PanelSession {
  return {
    id: nextId("notifications"),
    stack: [
      {
        id: "notifications.list",
        chrome: {
          title: "Notifications",
          hints: ["Up/Down move  Enter dismiss  Esc close"],
        },
        interaction: "menu",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "notifications",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createHotkeysPanelSession(): PanelSession {
  return {
    id: nextId("hotkeys"),
    stack: [
      {
        id: "hotkeys.list",
        chrome: {
          title: "Keybindings",
          hints: ["Up/Down scroll  Esc close"],
        },
        interaction: "menu",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "hotkeys",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createChangelogPanelSession(): PanelSession {
  return {
    id: nextId("changelog"),
    stack: [
      {
        id: "changelog.list",
        chrome: {
          title: "Changelog",
          hints: ["Up/Down scroll  Esc close"],
        },
        interaction: "menu",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "changelog",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  };
}

export function createImportSessionPanelSession(): PanelSession {
  return {
    id: nextId("import"),
    stack: [
      {
        id: "import.form",
        chrome: {
          title: "Import Session",
          hints: ["Enter submit  Esc close"],
        },
        interaction: "form",
        capabilities: [],
        body: {
          type: "session-import",
          payload: {},
        },
      },
    ],
    state: {},
  };
}
