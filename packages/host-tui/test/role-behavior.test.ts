// ============================================================================
// Role behavior unit tests — verify semantic key mapping for each role
// ============================================================================

import { describe, expect, it } from "bun:test";
import {
  confirmBehavior,
  formBehavior,
  menuBehavior,
  selectorBehavior,
} from "../src/surfaces/index.js";

describe("Role Keyboard Contracts", () => {
  describe("selectorBehavior", () => {
    it("Down arrow moves selection down and returns handled", () => {
      const state = { query: "", selectedIndex: 0 };
      const { nextState, result } = selectorBehavior({ name: "down" } as any, state, 5);
      expect(nextState.selectedIndex).toBe(1);
      expect(result.type).toBe("handled");
    });

    it("Up arrow moves selection up and returns handled", () => {
      const state = { query: "", selectedIndex: 2 };
      const { nextState, result } = selectorBehavior({ name: "up" } as any, state, 5);
      expect(nextState.selectedIndex).toBe(1);
      expect(result.type).toBe("handled");
    });

    it("Enter returns confirm", () => {
      const state = { query: "", selectedIndex: 1 };
      const { result } = selectorBehavior({ name: "enter" } as any, state, 5);
      expect(result.type).toBe("confirm");
    });

    it("Escape returns close", () => {
      const state = { query: "", selectedIndex: 1 };
      const { result } = selectorBehavior({ name: "escape" } as any, state, 5);
      expect(result.type).toBe("close");
    });

    it("Other key returns unhandled", () => {
      const state = { query: "", selectedIndex: 1 };
      const { result } = selectorBehavior({ name: "x" } as any, state, 5);
      expect(result.type).toBe("unhandled");
    });

    it("can skip non-selectable indices", () => {
      const state = { query: "", selectedIndex: 0 };
      const { nextState, result } = selectorBehavior({ name: "down" } as any, state, 5, {
        isSelectableIndex: (index) => index === 0 || index === 4,
      });
      expect(nextState.selectedIndex).toBe(4);
      expect(result.type).toBe("handled");
    });
  });

  describe("menuBehavior", () => {
    it("Enter returns confirm, Escape returns close", () => {
      const state = { query: "", selectedIndex: 0 };
      const resEnter = menuBehavior({ name: "enter" } as any, state, 5);
      expect(resEnter.result.type).toBe("confirm");
      const resEscape = menuBehavior({ name: "escape" } as any, state, 5);
      expect(resEscape.result.type).toBe("close");
    });
  });

  describe("formBehavior", () => {
    it("Typing letters appends to value", () => {
      const state = { value: "abc" };
      const { nextState, result } = formBehavior({ char: "d" } as any, state);
      expect(nextState.value).toBe("abcd");
      expect(result.type).toBe("handled");
    });

    it("Backspace deletes character", () => {
      const state = { value: "abc" };
      const { nextState, result } = formBehavior({ name: "backspace" } as any, state);
      expect(nextState.value).toBe("ab");
      expect(result.type).toBe("handled");
    });

    it("Enter returns submit with value", () => {
      const state = { value: "submit-text" };
      const { result } = formBehavior({ name: "enter" } as any, state);
      expect(result).toEqual({ type: "submit", value: "submit-text" });
    });

    it("Escape returns close", () => {
      const state = { value: "abc" };
      const { result } = formBehavior({ name: "escape" } as any, state);
      expect(result.type).toBe("close");
    });
  });

  describe("confirmBehavior", () => {
    it("Left/Right/Tab arrows switch active option", () => {
      const state = { activeOption: "confirm" as const };
      const resTab = confirmBehavior({ name: "tab" } as any, state);
      expect(resTab.nextState.activeOption).toBe("cancel");
      expect(resTab.result.type).toBe("handled");

      const resRight = confirmBehavior({ name: "right" } as any, resTab.nextState);
      expect(resRight.nextState.activeOption).toBe("confirm");
    });

    it("Enter on confirm returns confirm, Enter on cancel returns close", () => {
      const stateConf = { activeOption: "confirm" as const };
      const resConf = confirmBehavior({ name: "enter" } as any, stateConf);
      expect(resConf.result.type).toBe("confirm");

      const stateCancel = { activeOption: "cancel" as const };
      const resCancel = confirmBehavior({ name: "enter" } as any, stateCancel);
      expect(resCancel.result.type).toBe("close");
    });

    it("Escape returns close", () => {
      const state = { activeOption: "confirm" as const };
      const { result } = confirmBehavior({ name: "escape" } as any, state);
      expect(result.type).toBe("close");
    });
  });
});
