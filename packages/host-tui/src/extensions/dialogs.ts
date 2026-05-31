import {
  type Component,
  Input,
  type OverlayOptions,
  type SelectItem,
  SelectList,
  type TUI,
} from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";

type DialogPlacementOptions = {
  overlay?: boolean;
  overlayOptions?: OverlayOptions;
  showReplacement?: (component: Component, focusTarget?: Component) => { hide(): void };
};

function borderLine(theme: Theme, width: number): string {
  return theme.fg("border", "─".repeat(Math.max(1, width)));
}

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
  options?: DialogPlacementOptions,
): Promise<string | undefined> {
  return new Promise<string | undefined>((resolve) => {
    let handle: { hide(): void } | undefined;
    const list = new SelectList(items, Math.min(items.length, 15), getSelectListTheme());
    list.onSelect = (item) => {
      handle?.hide();
      resolve(item.value);
    };
    list.onCancel = () => {
      handle?.hide();
      resolve(undefined);
    };

    const innerWidth = Math.max(24, (process.stdout.columns ?? 80) - 8);
    const component = {
      render: (w: number) => {
        const iw = Math.min(w, innerWidth);
        return [
          borderLine(theme, w),
          theme.fg("accent", theme.bold(`  ${title}`)),
          "",
          ...list.render(iw),
          "",
          theme.fg("dim", "  ↑↓ navigate  ↵ select  Esc cancel"),
          borderLine(theme, w),
        ];
      },
      invalidate: () => list.invalidate(),
      handleInput: (data: string) => {
        list.handleInput(data);
        tui.requestRender();
      },
    };
    handle =
      options?.overlay === true || !options?.showReplacement
        ? tui.showOverlay(
            component,
            options?.overlayOptions ?? { anchor: "center", width: "60%", maxHeight: "50%" },
          )
        : options.showReplacement(component);
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
  options?: DialogPlacementOptions,
): Promise<boolean> {
  const items: SelectItem[] = [
    { value: "yes", label: "Yes" },
    { value: "no", label: "No" },
  ];

  return new Promise<boolean>((resolve) => {
    let handle: { hide(): void } | undefined;
    const list = new SelectList(items, 2, getSelectListTheme());
    list.onSelect = (item) => {
      handle?.hide();
      resolve(item.value === "yes");
    };
    list.onCancel = () => {
      handle?.hide();
      resolve(false);
    };

    const component = {
      render: (w: number) => [
        borderLine(theme, w),
        theme.fg("accent", theme.bold(`  ${title}`)),
        "",
        `  ${message}`,
        "",
        ...list.render(w - 4),
        "",
        theme.fg("dim", "  ↵ confirm  Esc cancel"),
        borderLine(theme, w),
      ],
      invalidate: () => list.invalidate(),
      handleInput: (data: string) => {
        list.handleInput(data);
        tui.requestRender();
      },
    };
    handle =
      options?.overlay === true || !options?.showReplacement
        ? tui.showOverlay(
            component,
            options?.overlayOptions ?? { anchor: "center", width: "50%", maxHeight: "30%" },
          )
        : options.showReplacement(component);
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
  options?: DialogPlacementOptions,
): Promise<string | undefined> {
  return new Promise<string | undefined>((resolve) => {
    let handle: { hide(): void } | undefined;
    const inp = new Input();
    if (placeholder) inp.setValue(placeholder);
    inp.onSubmit = (value) => {
      handle?.hide();
      resolve(value);
    };
    inp.onEscape = () => {
      handle?.hide();
      resolve(undefined);
    };

    const component = {
      render: (w: number) => {
        const iw = Math.max(24, w - 8);
        const inputLines = inp.render(iw);
        return [
          borderLine(theme, w),
          theme.fg("accent", theme.bold(`  ${title}`)),
          "",
          ...inputLines.map((l) => `  ${l}`),
          "",
          theme.fg("dim", "  ↵ submit  Esc cancel"),
          borderLine(theme, w),
        ];
      },
      invalidate: () => inp.invalidate(),
      handleInput: (data: string) => {
        inp.handleInput(data);
        tui.requestRender();
      },
    };
    handle =
      options?.overlay === true || !options?.showReplacement
        ? tui.showOverlay(
            component,
            options?.overlayOptions ?? { anchor: "center", width: "60%", maxHeight: "20%" },
          )
        : options.showReplacement(component, inp);
  });
}
