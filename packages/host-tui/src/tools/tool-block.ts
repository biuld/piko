import { Box, type Component, Container, Spacer, Text, type TUI } from "@earendil-works/pi-tui";
import { getTheme, type Theme } from "../theme.js";
import type { ToolDef, ToolRenderContext } from "./types.js";

/**
 * Tool execution block — copied from pi's ToolExecutionComponent,
 * adapted for piko's simplified ToolDef (render only, no execute).
 *
 * Features:
 * - Themed background (toolPendingBg / toolSuccessBg / toolErrorBg)
 * - Collapsed by default, toggle with expand/collapse
 * - Custom renderCall / renderResult callbacks from ToolDef
 */
export class ToolBlock extends Container {
  private contentBox: Box;
  private contentText: Text;
  private selfRenderContainer: Container;
  private callRendererComponent?: Component;
  private resultRendererComponent?: Component;
  private rendererState: any = {};
  private toolName: string;
  private toolCallId: string;
  private args: any;
  private expanded = false;
  private isPartial = true;
  private toolDef?: ToolDef;
  private ui: TUI;
  private cwd: string;
  private executionStarted = false;
  private argsComplete = false;
  private result?: {
    content: any;
    details?: any;
    isError: boolean;
  };

  constructor(
    toolName: string,
    toolCallId: string,
    args: any,
    toolDef: ToolDef | undefined,
    ui: TUI,
    cwd: string,
  ) {
    super();
    const t = getTheme();
    this.toolName = toolName;
    this.toolCallId = toolCallId;
    this.args = args;
    this.toolDef = toolDef;
    this.ui = ui;
    this.cwd = cwd;

    this.addChild(new Spacer(1));

    const pendingBg = (text: string) => t.bg("toolPendingBg", text);
    this.contentBox = new Box(1, 1, pendingBg);
    this.contentText = new Text("", 1, 1, pendingBg);
    this.selfRenderContainer = new Container();

    if (toolDef) {
      this.addChild(toolDef.renderShell === "self" ? this.selfRenderContainer : this.contentBox);
    } else {
      this.addChild(this.contentText);
    }

    this.updateDisplay();
  }

  private getRenderContext(lastComponent: Component | undefined): ToolRenderContext {
    return {
      args: this.args,
      toolCallId: this.toolCallId,
      invalidate: () => {
        this.invalidate();
        this.ui.requestRender();
      },
      lastComponent,
      state: this.rendererState,
      cwd: this.cwd,
      executionStarted: this.executionStarted,
      argsComplete: this.argsComplete,
      isPartial: this.isPartial,
      expanded: this.expanded,
      isError: this.result?.isError ?? false,
    };
  }

  updateArgs(args: any): void {
    this.args = args;
    this.updateDisplay();
  }

  markExecutionStarted(): void {
    this.executionStarted = true;
    this.updateDisplay();
    this.ui.requestRender();
  }

  setArgsComplete(): void {
    this.argsComplete = true;
    this.updateDisplay();
    this.ui.requestRender();
  }

  updateResult(result: { content: any; details?: any; isError: boolean }, isPartial = false): void {
    this.result = result;
    this.isPartial = isPartial;
    this.updateDisplay();
  }

  setExpanded(expanded: boolean): void {
    this.expanded = expanded;
    this.updateDisplay();
  }

  toggle(): void {
    this.expanded = !this.expanded;
    this.updateDisplay();
  }

  get isCollapsed(): boolean {
    return !this.expanded;
  }

  override invalidate(): void {
    super.invalidate();
    this.updateDisplay();
  }

  private updateDisplay(): void {
    const t = getTheme();
    const bgFn = this.isPartial
      ? (text: string) => t.bg("toolPendingBg", text)
      : this.result?.isError
        ? (text: string) => t.bg("toolErrorBg", text)
        : (text: string) => t.bg("toolSuccessBg", text);

    if (this.toolDef) {
      const renderContainer =
        this.toolDef.renderShell === "self" ? this.selfRenderContainer : this.contentBox;
      if (renderContainer instanceof Box) {
        renderContainer.setBgFn(bgFn);
      }
      renderContainer.clear();

      // Call renderer
      if (this.toolDef.renderCall) {
        try {
          const component = this.toolDef.renderCall(
            this.args,
            t as Theme,
            this.getRenderContext(this.callRendererComponent),
          );
          this.callRendererComponent = component;
          renderContainer.addChild(component);
        } catch {
          this.callRendererComponent = undefined;
          renderContainer.addChild(new Text(t.fg("toolTitle", t.bold(this.toolName)), 0, 0));
        }
      } else {
        renderContainer.addChild(new Text(t.fg("toolTitle", t.bold(this.toolName)), 0, 0));
      }

      // Result renderer (only when expanded)
      if (this.result && this.expanded && this.toolDef.renderResult) {
        try {
          const component = this.toolDef.renderResult(
            { content: this.result.content, details: this.result.details },
            { expanded: this.expanded, isPartial: this.isPartial },
            t as Theme,
            this.getRenderContext(this.resultRendererComponent),
          );
          this.resultRendererComponent = component;
          renderContainer.addChild(component);
        } catch {
          this.resultRendererComponent = undefined;
        }
      }
    } else {
      // No tool def — plain text fallback
      this.contentText.setCustomBgFn(bgFn);
      this.contentText.setText(this.formatPlain());
    }
  }

  private formatPlain(): string {
    const t = getTheme();
    let text = t.fg("toolTitle", t.bold(this.toolName));
    const argsStr = JSON.stringify(this.args, null, 2);
    if (argsStr && argsStr !== "{}") {
      text += `\n${argsStr}`;
    }
    if (this.result && this.expanded) {
      const output =
        typeof this.result.content === "string"
          ? this.result.content
          : JSON.stringify(this.result.content, null, 2);
      if (output) text += `\n${output}`;
    }
    return text;
  }
}
