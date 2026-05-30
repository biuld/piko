import {
  type Component,
  type Focusable,
  Input,
  truncateToWidth,
  visibleWidth,
} from "@earendil-works/pi-tui";

function padLine(line: string, width: number): string {
  const truncated = truncateToWidth(line, width, "…");
  const padding = Math.max(0, width - visibleWidth(truncated));
  return truncated + " ".repeat(padding);
}

export class PromptOverlay implements Component, Focusable {
  focused = false;
  private title: string;
  private hint: string;
  private input: Input;

  constructor(
    title: string,
    initialValue: string,
    hint: string,
    onSubmit: (value: string) => void,
    onCancel: () => void,
  ) {
    this.title = title;
    this.hint = hint;
    this.input = new Input();
    this.input.setValue(initialValue);
    this.input.onSubmit = onSubmit;
    this.input.onEscape = onCancel;
  }

  handleInput(data: string): void {
    this.input.handleInput(data);
  }

  invalidate(): void {
    this.input.invalidate();
  }

  render(width: number): string[] {
    const innerWidth = Math.max(24, width - 4);
    const inputLines = this.input.render(innerWidth);
    return [
      `┌${"─".repeat(innerWidth + 2)}┐`,
      `│ ${padLine(this.title, innerWidth)} │`,
      `├${"─".repeat(innerWidth + 2)}┤`,
      ...inputLines.map((line) => `│ ${padLine(line, innerWidth)} │`),
      `├${"─".repeat(innerWidth + 2)}┤`,
      `│ ${padLine(this.hint, innerWidth)} │`,
      `└${"─".repeat(innerWidth + 2)}┘`,
    ];
  }
}
