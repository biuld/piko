# Design: Interactive Login Overlay

This design document outlines the architecture, layout, and UX flows for introducing an interactive login and API key configuration menu inside the `piko-tui` terminal user interface.

## User Experience Flow

Currently, typing `/login` in `piko` immediately launches an OAuth login flow for `anthropic`, which fails because Anthropic OAuth is unsupported. The new interactive login flow will present a keyboard-navigable menu instead:

1. **Invoke `/login`**: The user types `/login` (without arguments) or selects "Login" from the Command Palette.
2. **Auth Type Selection**: An overlay panel appears, displaying two top-level options:
   * **Use a subscription (OAuth)**
   * **Use an API key**
3. **Provider Selection**:
   * If **Use a subscription (OAuth)** is selected, the list of providers supporting OAuth (e.g., `openai`) is displayed.
   * If **Use an API key** is selected, the list of all available providers (e.g., `anthropic`, `openai`, `gemini`, `deepseek`) is displayed.
4. **Credential Input / Execution**:
   * Selecting an OAuth provider triggers the standard device authorization flow: a verification URL and user code are displayed in the timeline, and the browser is launched.
   * Selecting an API key provider switches the overlay to an **API Key Input Prompt**. The user types or pastes their API key (which is visually masked with `*` or `•` for security) and presses Enter.
   * On submit, the TUI sends `Command::AuthSetApiKey { provider, api_key }` to `hostd` and closes the overlay.

## Architectural Changes

We will introduce a new `AppMode::AuthSelector` overlay state and a corresponding `AuthSelector` panel to handle this flow.

```
                    ┌────────────────────────┐
                    │        AppState        │
                    └───────────┬────────────┘
                                │ owns & controls
                                ▼
                    ┌────────────────────────┐
                    │      AuthSelector      │
                    └───────────┬────────────┘
                                │ manages
                                ▼
         ┌──────────────────────────────────────────────┐
         │                                              │
         ▼                                              ▼
┌────────────────────────┐                    ┌────────────────────────┐
│    HierarchicalMenu    │                    │     ApiKeyInputSub     │
│ (Select type & prov)   │                    │ (Input key masked)     │
└────────────────────────┘                    └────────────────────────┘
```

### 1. Mode and Placement Updates

We update `AppMode` in `packages/tui/src/app/mod.rs`:

```rust
pub enum AppMode {
    Chat,
    Sessions,
    Tree,
    Models,
    Settings,
    Status,
    Help,
    Approval,
    ToolInteraction,
    SummaryPrompt,
    AuthSelector, // <-- New Mode
}
```

Its placement is defined as `Placement::Partial` in `AppMode::placement()`:
```rust
AppMode::AuthSelector => Some(Placement::Partial)
```

### 2. Panel Definition (`packages/tui/src/features/auth_selector/mod.rs`)

We create a new module `auth_selector` under `packages/tui/src/features/`.
This panel encapsulates a `HierarchicalMenu<AuthAction>` for navigation and a simple state machine for the API key text entry phase.

```rust
#[derive(Clone, Debug)]
pub enum AuthAction {
    StartOAuth { provider: String },
    PromptApiKey { provider: String },
}

pub enum AuthSelectorState {
    Menu,
    ApiKeyInput {
        provider: String,
        input: String,
    },
}

pub struct AuthSelector {
    pub state: AuthSelectorState,
    pub menu: HierarchicalMenu<AuthAction>,
}
```

### 3. Rendering and Focus Details

* **Menu Phase**: The hierarchical menu is rendered using `HierarchicalMenu::render`.
* **API Key Phase**: The user is presented with a text input prompt:
  * Prompt: `Enter API key for <provider>:`
  * The characters typed are masked using `*`.
  * Keyboard navigation keys (`Enter` to submit, `Esc` to go back to the menu, and typing keys) are routed to updating `api_key_input`.

### 4. Slash Command updates

In `packages/tui/src/app/slash.rs` and `packages/tui/src/app/dispatch.rs`, the `/login` slash command (without arguments) will no longer trigger OAuth for `anthropic` immediately. Instead, it will:
1. Send a `Command::ModelList` request to `hostd` to ensure we have the latest provider list.
2. Open the `AuthSelector` panel in `AppMode::AuthSelector`.
3. If `/login <provider>` is explicitly typed (e.g. `/login openai`), it will immediately trigger `Command::AuthLoginOAuth` for that provider, bypassing the menu.

## Security Considerations

Since API keys are sensitive, the API key input field MUST mask input characters (e.g., displaying `*` for each character in the input string) during rendering to prevent shoulder-surfing.
