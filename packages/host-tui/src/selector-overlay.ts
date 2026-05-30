import {
  type Component,
  type Focusable,
  getKeybindings,
  Input,
  SelectList,
  type SelectItem,
  truncateToWidth,
  visibleWidth,
} from "@earendil-works/pi-tui";
import { getEditorTheme } from "./theme.js";

function padLine(line: string, width: number): string {
  const truncated = truncateToWidth(line, width, "…");
  const padding = Math.max(0, width - visibleWidth(truncated));
  return truncated + " ".repeat(padding);
}

export class SelectorOverlay implements Component, Focusable {
  private _focused = false;
  private selectList: SelectList;
  private input: Input;
  private items: SelectItem[];
  private title: string;
  private hint: string;
  private onSelect: (item: SelectItem) => void;
  private onCancel: () => void;
  private onInput?: (data: string) => boolean;
  private footerLines: string[];

  constructor(
    title: string,
    items: SelectItem[],
    hint: string,
    onSelect: (item: SelectItem) => void,
    onCancel: () => void,
    onInput?: (data: string) => boolean,
  ) {
    this.title = title;
    this.hint = hint;
    this.footerLines = [hint];
    this.items = items;
    this.onSelect = onSelect;
    this.onCancel = onCancel;
    this.onInput = onInput;
    this.input = new Input();
    this.input.onEscape = onCancel;
    this.selectList = this.createSelectList(items);
    this.input.onSubmit = () => {
      const selected = this.selectList.getSelectedItem();
      if (selected) {
        onSelect(selected);
      }
    };
  }

  get focused(): boolean {
    return this._focused;
  }

  set focused(value: boolean) {
    this._focused = value;
    this.input.focused = value;
  }

  handleInput(data: string): void {
    if (this.onInput?.(data)) {
      return;
    }

    const kb = getKeybindings();
    if (
      kb.matches(data, "tui.select.up")
      || kb.matches(data, "tui.select.down")
      || kb.matches(data, "tui.select.confirm")
      || kb.matches(data, "tui.select.cancel")
    ) {
      this.selectList.handleInput(data);
      return;
    }

    this.input.handleInput(data);
    this.applyFilter();
  }

  invalidate(): void {
    this.input.invalidate();
    this.selectList.invalidate();
  }

  render(width: number): string[] {
    const innerWidth = Math.max(24, width - 4);
    const inputLines = this.input.render(innerWidth);
    const listLines = this.selectList.render(innerWidth);
    const lines = [
      `┌${"─".repeat(innerWidth + 2)}┐`,
      `│ ${padLine(this.title, innerWidth)} │`,
      `├${"─".repeat(innerWidth + 2)}┤`,
      ...inputLines.map((line) => `│ ${padLine(line, innerWidth)} │`),
      `├${"─".repeat(innerWidth + 2)}┤`,
      ...listLines.map((line) => `│ ${padLine(line, innerWidth)} │`),
      `├${"─".repeat(innerWidth + 2)}┤`,
      ...this.footerLines.map((line) => `│ ${padLine(line, innerWidth)} │`),
      `└${"─".repeat(innerWidth + 2)}┘`,
    ];
    return lines;
  }

  setItems(items: SelectItem[]): void {
    this.items = items;
    this.applyFilter();
  }

  setTitle(title: string): void {
    this.title = title;
  }

  setHint(hint: string): void {
    this.hint = hint;
    this.footerLines = [hint];
  }

  setFooterLines(lines: string[]): void {
    this.footerLines = lines.length > 0 ? lines : [this.hint];
  }

  getSelectedValue(): string | null {
    return this.selectList.getSelectedItem()?.value ?? null;
  }

  private applyFilter(): void {
    const query = this.input.getValue().trim().toLowerCase();
    const filteredItems = query.length === 0
      ? this.items
      : this.items.filter((item) => {
        const haystack = [item.label, item.description, item.value].filter(Boolean).join(" ").toLowerCase();
        return haystack.includes(query);
      });
    this.selectList = this.createSelectList(filteredItems);
  }

  private createSelectList(items: SelectItem[]): SelectList {
    const selectList = new SelectList(items, 10, getEditorTheme().selectList);
    selectList.onSelect = this.onSelect;
    selectList.onCancel = this.onCancel;
    return selectList;
  }
}
