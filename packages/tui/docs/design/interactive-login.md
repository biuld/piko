# Design: Interactive Login Overlay

> Status: implemented

This document records the architecture, layout, and UX flow of the interactive login and API key configuration menu in the TUI.

## User Experience Flow

Previously, typing `/login` immediately launched an OAuth flow for `anthropic`, which failed because Anthropic OAuth is unsupported. The implemented flow presents a keyboard-navigable menu instead:

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

`AppMode::AuthSelector` and the corresponding `AuthSelector` panel own this flow.

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

`AppMode` includes the authentication selector:

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

The `auth_selector` feature module encapsulates the menu and API-key entry state machine.
This panel encapsulates a `HierarchicalMenu<AuthAction>` for navigation and a simple state machine for the API key text entry phase.

```rust
#[derive(Clone, Debug)]
pub enum AuthAction {
    StartOAuth { provider: String },
    StartApiKey { provider: String },
}

pub enum AuthSelectorState {
    Menu,
    ApiKeyInput {
        provider: String,
        input: TextBox,
    },
}

pub struct AuthSelector {
    pub state: AuthSelectorState,
    pub menu: HierarchicalMenu<AuthAction>,
    pub filter: String,
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
