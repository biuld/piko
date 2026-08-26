# Design: Auto-completion

This design doc derives from the Feature Spec under `packages/tui/docs/features/auto-completion.md`.

## Architecture Overview

We refactor the auto-completion logic into a trait-based provider architecture under `packages/tui/src/features/auto_completion/`. 

This design establishes:
1. **Generic Item Representation (`CompletionRow`)**: Sub-feature agnostic shape representing rendering columns and replacement action.
2. **Provider Trait (`AutoCompleteProvider`)**: Encapsulates data fetching, filtering, custom key actions, and titles.
3. **AutoComplete Controller**: Manages active state, selection index, provider switching, and delegates rendering.

```
                  ┌──────────────────────────────┐
                  │            Editor            │
                  └──────────────┬───────────────┘
                                 │ owns & delegates
                                 ▼
                  ┌──────────────────────────────┐
                  │         AutoComplete         │
                  └──────────────┬───────────────┘
                                 │ manages
                                 ▼
                  ┌──────────────────────────────┐
                  │    AutoCompleteProvider      │
                  │           (Trait)            │
                  └──────┬────────────────┬──────┘
                         │                │
                         ▼                ▼
             ┌───────────────────┐┌───────────────────┐
             │  SlashCommands    ││    FileBrowser    │
             │    (Slash /)      ││     (File @)      │
             └───────────────────┘└───────────────────┘
```

> `commands: &[TuiCommandEntry]` is the TUI-local merge of hostd's neutral
> `HostCommandDescriptor` catalog with TUI-local presentation commands
> (`app::command::merge_command_catalog`). Slash aliases live only in this
> merge, never on the wire.

## Module Structure

We organize code under `packages/tui/src/features/auto_completion/`:
* `mod.rs`: Defines the `AutoComplete` controller, `CompletionRow`, `CompletionCell`, and integration logic.
* `provider.rs`: Defines the `AutoCompleteProvider` trait and enum result types.
* `slash_commands.rs`: Implements `AutoCompleteProvider` for slash commands.
* `file_browser.rs`: Implements `AutoCompleteProvider` for local file system browsing.

## Types and Trait Specifications

### 1. Generic Shape Types
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRow {
    /// Replaced text in the Editor.
    pub replacement: String,
    pub start: usize,
    pub end: usize,
    
    /// Visual representation cells (e.g. Column 0: label, Column 1: details/type/size).
    pub cells: Vec<CompletionCell>,
    
    /// If true, accepting this item keeps the autocomplete panel open.
    pub keep_active: bool,
    /// When true, accept also submits the editor (Immediate slash commands).
    pub submit_on_accept: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCell {
    pub text: String,
    /// Shared with selectable Columns body (`Primary` | `Secondary`).
    pub style: ColumnCellStyle,
}
```

### 2. AutoCompleteProvider Trait
```rust
pub trait AutoCompleteProvider {
    /// String prefix that triggers this provider (e.g. "/" or "@").
    fn trigger(&self) -> &str;

    /// Checks if this provider is triggered by the current token.
    fn is_triggered(&self, text: &str, cursor: usize) -> bool;

    /// Fetches and filters completion items.
    fn update(
        &mut self,
        cwd: &Path,
        commands: &[TuiCommandEntry],
        text: &str,
        cursor: usize,
    ) -> Vec<CompletionRow>;

    /// Title displayed in the Suggestions block header.
    fn title(&self, selected: usize, total: usize) -> String;
}
```

### 3. AutoComplete Controller
```rust
pub struct AutoComplete {
    pub active: bool,
    pub items: Vec<CompletionRow>,
    pub selected: usize,
    pub active_provider_idx: Option<usize>,
    pub providers: Vec<Box<dyn AutoCompleteProvider>>,
}
```

## Key Workflows

### Triggering & Provider Switching
During `AutoComplete::update`, the controller iterates through its registered `providers`. 
- It finds the provider where `provider.is_triggered(text, cursor)` returns true.
- If one matches, it sets `active_provider_idx` to its index, sets `active = true`, and delegates the query retrieval to `provider.update()`.
- If none matches, it clears state and sets `active = false`.

The file provider only detects and describes the active query in this path.
Filesystem traversal runs on the completion worker and returns a generation-
tagged result. The app reducer accepts the result only when both generation and
current trigger still match. Results are ranked deterministically by fuzzy
score and path before the display cap is applied. Worker requests and results
use the declared `piko-comms` TUI file-search thread-bridge contracts.

### Multi-Column Table Rendering

### Dock boundary chrome

When Suggest is the first optional Dock Stack band, `Region::DockBoundary`
paints the provider label and selection affix. `Region::Suggest` then uses a
bottom-only `PaneSpec`, so its first row is content rather than a duplicate top
rule. If Todos precedes Suggest, Suggest retains its normal Minimal top/bottom
chrome because the shared boundary is no longer adjacent.

Height offers mirror paint: shared-boundary Suggest reserves content + bottom;
non-shared Suggest reserves top + content + bottom. Pointer row projection uses
the same chrome mode as rendering.
In `AutoComplete::render()`, we dynamically compute the maximum width of each column (except the last one) across all items in `self.items`.
Rows are rendered with ratatui's `Table` widget, using a small marker column for the selected row and provider-defined cells for the completion columns. This allows multi-column layouts like:
```
> @src/main.rs        file        1.2 KB        rw-r--r--
```
This is fully sub-feature agnostic.

### Accept paths

| Input | Action | Result |
|-------|--------|--------|
| Enter (suggestions active) | `AcceptAndSubmitSuggestion` | `accept_suggestion()` then `submit()` if `submit_on_accept` |
| Pointer activate on a row | select that index, then `AcceptAndSubmitSuggestion` | same as Enter |
| Tab | `SuggestionSelectNext` only | cycle; no insert-submit |
| Esc | `CancelSuggestions` | clear list, leave editor text |

Slash provider sets `submit_on_accept` from
`HostCommandInvoke::Immediate`. File provider always sets `false`.

Click must not use bare `AcceptSuggestion` for slash Immediate commands — that
would only fill the composer without executing the command.
