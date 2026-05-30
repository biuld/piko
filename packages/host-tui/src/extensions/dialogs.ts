import { Input, type SelectItem, SelectList, type TUI } from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";

/**
 * Show a selection dialog.
 * Returns the selected item's value, or undefined if cancelled.
 */
export async function showSelectDialog(
  tui: TUI,
  theme: Theme,
  title: string,
  items: SelectItem[],
  getSelectListTheme: () => import("@earendil-works/pi-tui").SelectListTheme,
  overlayOptions?: import("@earendil-works/pi-tui").OverlayOptions,
): Promise<string | undefined> {
  return new Promise<string | undefined>((resolve) => {
    let overlayHandle: { hide(): void } | undefined;
    const list = new SelectList(items, Math.min(items.length, 15), getSelectListTheme());
    list.onSelect = (item) => {
      overlayHandle?.hide();
      resolve(item.value);
    };
    list.onCancel = () => {
      overlayHandle?.hide();
      resolve(undefined);
    };

    const innerWidth = Math.max(24, (process.stdout.columns ?? 80) - 8);
    overlayHandle = tui.showOverlay(
      {
        render: (w: number) => {
          const iw = Math.min(w, innerWidth);
          return [
            theme.fg("accent", theme.bold(`  ${title}`)),
            "",
            ...list.render(iw),
            "",
            theme.fg("dim", "  ↑↓ navigate  ↵ select  Esc cancel"),
          ];
        },
        invalidate: () => list.invalidate(),
        handleInput: (data: string) => {
          list.handleInput(data);
          tui.requestRender();
        },
      },
      overlayOptions ?? { anchor: "center", width: "60%", maxHeight: "50%" },
    );
  });
}

/**
 * Show a yes/no confirmation dialog.
 */
export async function showConfirmDialog(
  tui: TUI,
  theme: Theme,
  title: string,
  message: string,
  getSelectListTheme: () => import("@earendil-works/pi-tui").SelectListTheme,
  overlayOptions?: import("@earendil-works/pi-tui").OverlayOptions,
): Promise<boolean> {
  const items: SelectItem[] = [
    { value: "yes", label: "Yes" },
    { value: "no", label: "No" },
  ];

  return new Promise<boolean>((resolve) => {
    let overlayHandle: { hide(): void } | undefined;
    const list = new SelectList(items, 2, getSelectListTheme());
    list.onSelect = (item) => {
      overlayHandle?.hide();
      resolve(item.value === "yes");
    };
    list.onCancel = () => {
      overlayHandle?.hide();
      resolve(false);
    };

    overlayHandle = tui.showOverlay(
      {
        render: (w: number) => [
          theme.fg("accent", theme.bold(`  ${title}`)),
          "",
          `  ${message}`,
          "",
          ...list.render(w - 4),
          "",
          theme.fg("dim", "  ↵ confirm  Esc cancel"),
        ],
        invalidate: () => list.invalidate(),
        handleInput: (data: string) => {
          list.handleInput(data);
          tui.requestRender();
        },
      },
      overlayOptions ?? { anchor: "center", width: "50%", maxHeight: "30%" },
    );
  });
}

/**
 * Show a single-line text input dialog.
 * Returns the entered text, or undefined if cancelled.
 */
export async function showInputDialog(
  tui: TUI,
  theme: Theme,
  title: string,
  placeholder?: string,
  overlayOptions?: import("@earendil-works/pi-tui").OverlayOptions,
): Promise<string | undefined> {
  return new Promise<string | undefined>((resolve) => {
    let overlayHandle: { hide(): void } | undefined;
    const inp = new Input();
    if (placeholder) inp.setValue(placeholder);
    inp.onSubmit = (value) => {
      overlayHandle?.hide();
      resolve(value);
    };
    inp.onEscape = () => {
      overlayHandle?.hide();
      resolve(undefined);
    };

    overlayHandle = tui.showOverlay(
      {
        render: (w: number) => {
          const iw = Math.max(24, w - 8);
          const inputLines = inp.render(iw);
          return [
            theme.fg("accent", theme.bold(`  ${title}`)),
            "",
            ...inputLines.map((l) => `  ${l}`),
            "",
            theme.fg("dim", "  ↵ submit  Esc cancel"),
          ];
        },
        invalidate: () => inp.invalidate(),
        handleInput: (data: string) => {
          inp.handleInput(data);
          tui.requestRender();
        },
      },
      overlayOptions ?? { anchor: "center", width: "60%", maxHeight: "20%" },
    );
  });
}
