import { describe, expect, it } from "bun:test";
import {
  appendListQuery,
  createSelectableListState,
  filterSelectableItems,
  getSelectableListWindow,
  handleSelectableListKey,
  moveListSelection,
} from "../src/surfaces/interactions/selectable-list.js";

describe("selectable-list interaction", () => {
  const items = [
    { id: "model", label: "/model", description: "Select a model", value: "model" },
    { id: "resume", label: "/resume", description: "Resume a session", value: "resume" },
    { id: "thinking", label: "/thinking", description: "Change thinking", value: "thinking" },
  ];

  it("clamps movement to available rows", () => {
    let state = createSelectableListState();
    state = moveListSelection(state, 3, 10);
    expect(state.selectedIndex).toBe(2);
    state = moveListSelection(state, 3, -10);
    expect(state.selectedIndex).toBe(0);
  });

  it("resets selection when query changes", () => {
    const selected = moveListSelection(createSelectableListState(), 3, 2);
    const queried = appendListQuery(selected, "m");
    expect(queried.query).toBe("m");
    expect(queried.selectedIndex).toBe(0);
  });

  it("filters by label and description", () => {
    expect(filterSelectableItems(items, "model").map((item) => item.id)).toEqual(["model"]);
    expect(filterSelectableItems(items, "session").map((item) => item.id)).toEqual(["resume"]);
  });

  it("computes a moving visible window", () => {
    const window = getSelectableListWindow(items, 2, 2);
    expect(window.start).toBe(1);
    expect(window.rows.map((item) => item.id)).toEqual(["resume", "thinking"]);
  });

  it("handles key events without renderer state", () => {
    const state = createSelectableListState();
    const next = handleSelectableListKey(
      state,
      { name: "down", ctrl: false, shift: false },
      { total: 3 },
    );
    expect(next?.selectedIndex).toBe(1);
  });

  it("computes an edge-anchored visible window", () => {
    const moreItems = ["0", "1", "2", "3", "4", "5"].map((id) => ({ id, label: id, value: id }));
    const window = getSelectableListWindow(moreItems, 4, 3, "edge");
    expect(window.start).toBe(3);
    expect(window.rows.map((item) => item.id)).toEqual(["3", "4", "5"]);
  });

  it("moves between selectable indices when a predicate is provided", () => {
    const state = createSelectableListState();
    const next = handleSelectableListKey(
      state,
      { name: "down", ctrl: false, shift: false },
      {
        total: 5,
        isSelectableIndex: (index) => index === 0 || index === 3,
      },
    );
    expect(next?.selectedIndex).toBe(3);
  });
});
