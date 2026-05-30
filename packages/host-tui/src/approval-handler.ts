import type { TUI } from "@earendil-works/pi-tui";
import { Container, type SelectItem, SelectList, Spacer, Text } from "@earendil-works/pi-tui";
import type { PendingApprovalState } from "piko-engine-protocol";
import type { ApprovalDecision, ApprovalHandler } from "piko-host-runtime";
import { DynamicBorder } from "./components/dynamic-border.js";
import { keyHint } from "./components/key-hints.js";
import { makeFocusable } from "./overlays/focusable.js";
import { getSelectListTheme, getTheme } from "./theme.js";

/**
 * Interactive approval handler that shows a TUI dialog for each approval request.
 */
export class InteractiveApprovalHandler implements ApprovalHandler {
  private tui: TUI;

  constructor(tui: TUI) {
    this.tui = tui;
  }

  async requestApproval(state: PendingApprovalState): Promise<ApprovalDecision> {
    const t = getTheme();
    const borderColor = (s: string) => t.fg("border", s);

    const kindLabel = typeof state.kind === "string" ? state.kind : "tool";
    const detailsStr =
      typeof state.details === "string" ? state.details : JSON.stringify(state.details, null, 2);

    return new Promise<ApprovalDecision>((resolve) => {
      const items: SelectItem[] = [
        { value: "accept", label: "Accept", description: "Approve and execute" },
        {
          value: "acceptForSession",
          label: "Accept for session",
          description: "Approve all future calls too",
        },
        { value: "decline", label: "Decline", description: "Skip this tool call" },
      ];

      const list = new SelectList(items, 3, getSelectListTheme());
      list.onSelect = (item) => {
        overlayHandle?.hide();
        resolve(item.value as ApprovalDecision);
      };
      list.onCancel = () => {
        overlayHandle?.hide();
        resolve("decline");
      };

      const container = new Container();
      container.addChild(new DynamicBorder(borderColor));
      container.addChild(new Text(t.fg("warning", t.bold(` Approve: ${kindLabel}`)), 1, 0));
      container.addChild(new Spacer(1));
      container.addChild(new Text(t.fg("muted", ` ${detailsStr.slice(0, 200)}`), 1, 0));
      container.addChild(new Spacer(1));
      container.addChild(list);
      container.addChild(new Spacer(1));
      container.addChild(
        new Text(
          `${keyHint("tui.select.confirm", "select")}  ${keyHint("tui.select.cancel", "decline")}`,
          1,
          0,
        ),
      );
      container.addChild(new DynamicBorder(borderColor));

      const component = makeFocusable(container);
      const self = this;
      Object.assign(component, {
        handleInput(data: string) {
          list.handleInput(data);
          self.tui.requestRender();
        },
      });

      const overlayHandle = this.tui.showOverlay(component, {
        anchor: "center",
        width: "60%",
        maxHeight: "50%",
      });
    });
  }
}
