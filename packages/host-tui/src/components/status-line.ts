import { type Component, Text } from "@earendil-works/pi-tui";

export class StatusLine implements Component {
  private text = new Text("");

  setStatus(message: string): void {
    this.text.setText(message);
  }

  clear(): void {
    this.text.setText("");
  }

  invalidate(): void {
    this.text.invalidate();
  }

  render(width: number): string[] {
    return this.text.render(width);
  }
}
