import type { PanelAction } from "./panel-actions.js";
import type { PanelRoute, PanelSession, PanelState } from "./types.js";

export class PanelRuntime {
  constructor(
    public session: PanelSession,
    private readonly onChange: () => void,
    private readonly onDismiss: () => void,
  ) {}

  get currentRoute(): PanelRoute<any> {
    return this.session.stack[this.session.stack.length - 1];
  }

  get state(): PanelState {
    return this.session.state;
  }

  dispatch(action: PanelAction): void {
    switch (action.type) {
      case "push_route":
        this.session.stack.push(action.route);
        this.onChange();
        break;

      case "pop_route":
        if (this.session.stack.length > 1) {
          this.session.stack.pop();
          this.onChange();
        } else {
          this.onDismiss();
        }
        break;

      case "replace_route":
        this.session.stack[this.session.stack.length - 1] = action.route;
        this.onChange();
        break;

      case "update_filter":
        this.session.state.filterText = action.text;
        // Reset selection when filter changes
        this.session.state.selectedIndex = 0;
        this.onChange();
        break;

      case "update_selection":
        this.session.state.selectedIndex = action.index;
        this.onChange();
        break;

      case "update_form":
        this.session.state.formValues = {
          ...this.session.state.formValues,
          ...action.values,
        };
        this.onChange();
        break;

      case "submit":
        // This is a generic submit, specific body components or domain logic might handle it differently.
        break;

      case "cancel":
        this.onDismiss();
        break;
    }
  }
}
