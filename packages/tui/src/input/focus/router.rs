use super::*;

impl InputRouter {
    /// Route a key event through the 3-layer priority chain:
    ///
    /// ```
    /// P1: Global Esc/Enter → handle_global_key()
    ///   ├─ Esc: Approval decline, close surface, cancel suggestions, cancel turn, open tree
    ///   └─ Enter: Approval accept, confirm selection, accept suggestion, submit
    ///
    /// P2: Focus Owner → handle_focus_key()
    ///   ├─ If Capture: keys consumed by panel, nothing reaches Editor
    ///   └─ If Passive: unhandled keys pass through to Editor
    ///
    /// P3: Editor → handle_editor_key()
    ///   └─ Text input, cursor movement, history, timeline scroll, keyboard commands
    /// ```
    pub fn route_key(app: &mut AppState, keymap: &Keymap, key: KeyEvent) -> Option<Action> {
        let ka = keymap.action_for(key);

        // ═══ P1: Global Esc/Enter ═══
        if let Some(action) = Self::handle_global_key(app, ka, key) {
            return Some(action);
        }

        // ── P1.5: Global keybindings that open select surfaces ──
        if ka == Some(KeyAction::AgentPanel) {
            // Modal authority: a pending Decide surface is a focus barrier.
            // Opening Agents here would desync focus from the drawn modal.
            if app.pending_decide().is_some() {
                return None;
            }
            return Some(SurfaceAction::OpenAgents.into());
        }

        // ── P2: Focus Owner ═══
        let active = app.focus_manager.active_mode();
        if active != AppMode::Chat {
            // Check if SummaryPrompt overrides
            if active.is_surface(SurfaceId::SummaryPrompt) {
                match key.code {
                    KeyCode::Esc => {
                        if let Some(state) = &mut app.summary_prompt
                            && state.input_active()
                        {
                            state.set_input_active(false);
                            return None;
                        }
                        app.summary_prompt = None;
                        app.pop_focus();
                        return None;
                    }
                    KeyCode::Enter => {
                        if app.summary_prompt.is_some() {
                            return Some(SurfaceAction::Confirm.into());
                        }
                    }
                    KeyCode::Up | KeyCode::Left | KeyCode::BackTab => {
                        return Some(SurfaceAction::SelectPrev.into());
                    }
                    KeyCode::Down | KeyCode::Right | KeyCode::Tab => {
                        return Some(SurfaceAction::SelectNext.into());
                    }
                    KeyCode::Backspace => return Some(SurfaceAction::FilterBackspace.into()),
                    KeyCode::Char(ch) => {
                        if ch == 'C' && key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.summary_prompt = None;
                            app.pop_focus();
                            return None;
                        }
                        if let Some(state) = &mut app.summary_prompt
                            && state.input_active()
                        {
                            return Some(SurfaceAction::FilterAppend(ch).into());
                        }
                    }
                    _ => {}
                }
                // Don't pass through if active
                return None;
            }

            if let Some(action) = Self::handle_focus_key(app, active, ka, key) {
                return Some(action);
            }
            // Capture panels: consume event, don't pass to Editor. All non-Chat
            // modes are treated as Capture today.
            return None;
        }

        // ═══ P3: Editor ═══
        Self::handle_editor_key(app, ka, key)
    }

    // ── P1: Global Esc/Enter ────────────────────────────────────────────────

    pub(super) fn handle_global_key(
        app: &mut AppState,
        ka: Option<KeyAction>,
        key: KeyEvent,
    ) -> Option<Action> {
        // ── Esc (Cancel) ──
        if ka == Some(KeyAction::Cancel) || key.code == KeyCode::Esc {
            if app
                .focus_manager
                .active_mode()
                .is_surface(SurfaceId::Approval)
            {
                return Some(ApprovalAction::Respond(ApprovalDecision::Decline).into());
            }
            if app
                .focus_manager
                .active_mode()
                .is_surface(SurfaceId::ToolInteraction)
            {
                return Some(ToolInteractionAction::Cancel.into());
            }
            // 1. Blocking surface active → close it
            if app.focus_manager.is_blocking_surface_active() {
                return Some(SurfaceAction::Close.into());
            }
            // 2. Suggestions visible → cancel them
            if app.has_suggestions() {
                return Some(EditorAction::CancelSuggestions.into());
            }
            // 3. Active turn → cancel it
            if app.active_turn_id().is_some() {
                return Some(EditorAction::Cancel.into());
            }
            // 4. Editor empty + double-Esc → open tree
            if app.editor.is_empty() {
                let now = Instant::now();
                let double_esc = app
                    .focus_manager
                    .last_esc_pressed
                    .map(|last| now.duration_since(last).as_millis() < 500)
                    .unwrap_or(false);
                if double_esc {
                    app.focus_manager.last_esc_pressed = None;
                    return Some(SurfaceAction::OpenTree.into());
                }
                app.focus_manager.last_esc_pressed = Some(now);
            }
            return None;
        }

        // ── Enter ──
        if key.code == KeyCode::Enter || ka == Some(KeyAction::Submit) {
            // Let P2 handle Enter if a surface is active
            // (handled below in handle_focus_key)
        }

        None
    }

    // ── P2: Focus Owner ─────────────────────────────────────────────────────

    pub(super) fn handle_focus_key(
        _app: &AppState,
        active: AppMode,
        ka: Option<KeyAction>,
        key: KeyEvent,
    ) -> Option<Action> {
        // Approval / tool-interaction / summary have dedicated capture paths.
        if active.is_surface(SurfaceId::Approval) {
            if let Some(action) = match ka {
                Some(KeyAction::ApprovalAccept) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::Accept).into())
                }
                Some(KeyAction::ApprovalAcceptSession) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptSession).into())
                }
                Some(KeyAction::ApprovalAcceptWorkspace) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptWorkspace).into())
                }
                Some(KeyAction::ApprovalDecline) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::Decline).into())
                }
                _ => None,
            } {
                return Some(action);
            }
            return match key.code {
                KeyCode::Enter => Some(ApprovalAction::Respond(ApprovalDecision::Accept).into()),
                KeyCode::Esc => Some(ApprovalAction::Respond(ApprovalDecision::Decline).into()),
                KeyCode::Char('a' | 'A')
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptSession).into())
                }
                KeyCode::Char('w' | 'W')
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptWorkspace).into())
                }
                KeyCode::Char('p' | 'P')
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptPermanent).into())
                }
                _ => None,
            };
        }

        if active.is_surface(SurfaceId::ToolInteraction) {
            return match key.code {
                KeyCode::Enter => Some(ToolInteractionAction::Submit.into()),
                KeyCode::Esc => Some(ToolInteractionAction::Cancel.into()),
                // Choice navigation: Up/Down move within the active question.
                KeyCode::Down => Some(ToolInteractionAction::SelectNext.into()),
                KeyCode::Up => Some(ToolInteractionAction::SelectPrev.into()),
                // Step navigation: Tab/Shift+Tab (and Left/Right) move between
                // questions and the Submit step.
                KeyCode::Tab | KeyCode::Right => Some(ToolInteractionAction::NextStep.into()),
                KeyCode::BackTab | KeyCode::Left => Some(ToolInteractionAction::PrevStep.into()),
                KeyCode::Backspace => Some(SurfaceAction::FilterBackspace.into()),
                KeyCode::Char(ch)
                    if ch.is_ascii_digit()
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    ch.to_digit(10)
                        .and_then(|digit| digit.checked_sub(1))
                        .map(|idx| ToolInteractionAction::Choice(idx as usize).into())
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(SurfaceAction::FilterAppend(ch).into())
                }
                _ => None,
            };
        }

        match active.as_surface() {
            // Selectable list surfaces (filter + keyboard nav)
            Some(
                surface @ (SurfaceId::Tree
                | SurfaceId::Sessions
                | SurfaceId::Agents
                | SurfaceId::Settings
                | SurfaceId::Models
                | SurfaceId::Thinking
                | SurfaceId::AuthSelector),
            ) => {
                if surface == SurfaceId::Tree {
                    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                        return Some(if key.modifiers.contains(KeyModifiers::SHIFT) {
                            TreeAction::FilterCycleBackward.into()
                        } else {
                            TreeAction::FilterCycleForward.into()
                        });
                    }
                    match ka {
                        Some(KeyAction::TreeFoldOrUp) => return Some(TreeAction::FoldOrUp.into()),
                        Some(KeyAction::TreeUnfoldOrDown) => {
                            return Some(TreeAction::UnfoldOrDown.into());
                        }
                        Some(KeyAction::TreeEditLabel) => {
                            return Some(TreeAction::EditLabel.into());
                        }
                        Some(KeyAction::TreeToggleLabelTimestamp) => {
                            return Some(TreeAction::ToggleLabelTimestamp.into());
                        }
                        Some(KeyAction::TreeFilterCycleForward) => {
                            return Some(TreeAction::FilterCycleForward.into());
                        }
                        Some(KeyAction::TreeFilterCycleBackward) => {
                            return Some(TreeAction::FilterCycleBackward.into());
                        }
                        _ => {
                            if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && key.code == KeyCode::Left
                            {
                                return Some(TreeAction::FoldOrUp.into());
                            } else if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && key.code == KeyCode::Right
                            {
                                return Some(TreeAction::UnfoldOrDown.into());
                            } else if key.code == KeyCode::Char('L')
                                && key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                return Some(TreeAction::EditLabel.into());
                            } else if key.code == KeyCode::Char('T')
                                && key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                return Some(TreeAction::ToggleLabelTimestamp.into());
                            } else if key.code == KeyCode::Char('o')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                if key.modifiers.contains(KeyModifiers::SHIFT) {
                                    return Some(TreeAction::FilterCycleBackward.into());
                                } else {
                                    return Some(TreeAction::FilterCycleForward.into());
                                }
                            }
                        }
                    }
                }

                if surface == SurfaceId::Sessions {
                    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                        return Some(SessionAction::ToggleScope.into());
                    }
                    if let Some(action) = ka {
                        match action {
                            KeyAction::SessionToggleNamedFilter => {
                                return Some(SessionAction::ToggleNamed.into());
                            }
                            KeyAction::SessionTogglePath => {
                                return Some(SessionAction::TogglePath.into());
                            }
                            _ => {}
                        }
                    }
                }
                Self::handle_selectable_surface(key, ka)
            }
            // Info panels
            Some(SurfaceId::Status | SurfaceId::Mcp | SurfaceId::Diagnostics) => match ka {
                Some(KeyAction::SelectPrev) => Some(SurfaceAction::SelectPrev.into()),
                Some(KeyAction::SelectNext) => Some(SurfaceAction::SelectNext.into()),
                Some(KeyAction::Submit | KeyAction::Confirm) => Some(SurfaceAction::Confirm.into()),
                Some(KeyAction::Cancel) => Some(SurfaceAction::Close.into()),
                Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
                None if matches!(key.code, KeyCode::Char('q')) => Some(SurfaceAction::Close.into()),
                _ => None,
            },
            // Handled above; fall through if focus somehow reached here.
            Some(SurfaceId::Approval | SurfaceId::ToolInteraction | SurfaceId::SummaryPrompt) => {
                None
            }
            // Editor chrome never reaches filter routing via this path.
            None => None,
        }
    }

    /// Shared logic for selectable list surfaces (Tree, Sessions, Settings, Models, …).
    pub(super) fn handle_selectable_surface(
        key: KeyEvent,
        ka: Option<KeyAction>,
    ) -> Option<Action> {
        // Character input → filter append
        if let KeyCode::Char(ch) = key.code
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
        {
            return Some(SurfaceAction::FilterAppend(ch).into());
        }
        // Backspace → filter backspace
        if key.code == KeyCode::Backspace {
            return Some(SurfaceAction::FilterBackspace.into());
        }
        // Keymap-driven actions
        match ka {
            Some(KeyAction::SelectPrev) => Some(SurfaceAction::SelectPrev.into()),
            Some(KeyAction::SelectNext) => Some(SurfaceAction::SelectNext.into()),
            Some(KeyAction::Submit | KeyAction::Confirm) => Some(SurfaceAction::Confirm.into()),
            Some(KeyAction::Cancel) => Some(SurfaceAction::Close.into()),
            Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
            None if matches!(key.code, KeyCode::Char('q')) => Some(SurfaceAction::Close.into()),
            _ => None,
        }
    }

    // ── P3: Editor ──────────────────────────────────────────────────────────

    pub(super) fn handle_editor_key(
        app: &AppState,
        ka: Option<KeyAction>,
        key: KeyEvent,
    ) -> Option<Action> {
        // Autocomplete intercepts Up/Down/Tab/Enter when suggestions are visible
        if app.has_suggestions() {
            match ka {
                Some(KeyAction::SelectPrev | KeyAction::TimelineUp) => {
                    return Some(EditorAction::SuggestionSelectPrev.into());
                }
                Some(KeyAction::SelectNext | KeyAction::TimelineDown) => {
                    return Some(EditorAction::SuggestionSelectNext.into());
                }
                Some(KeyAction::Complete) => {
                    return Some(EditorAction::SuggestionSelectNext.into());
                }
                Some(KeyAction::ThinkingCycle) => {
                    return Some(EditorAction::SuggestionSelectPrev.into());
                }
                Some(KeyAction::Submit) => {
                    return Some(EditorAction::AcceptAndSubmitSuggestion.into());
                }
                _ => {}
            }
        }

        // Standard editor inputs, timeline scroll, and keyboard commands
        match ka {
            Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
            Some(KeyAction::NewLine) => Some(EditorAction::InsertNewline.into()),
            Some(KeyAction::Sessions) => Some(SessionAction::RequestList.into()),
            Some(KeyAction::SessionTree) => Some(SurfaceAction::OpenTree.into()),
            Some(KeyAction::Commands) => Some(EditorAction::OpenCommands.into()),
            Some(KeyAction::Settings) => Some(SurfaceAction::OpenSettings.into()),
            Some(KeyAction::Status) => Some(SurfaceAction::OpenStatus.into()),
            Some(KeyAction::ClearNotifications) => Some(NotificationAction::Clear.into()),
            Some(KeyAction::HistoryPrev) => Some(EditorAction::HistoryPrev.into()),
            Some(KeyAction::HistoryNext) => Some(EditorAction::HistoryNext.into()),
            Some(KeyAction::DeleteBackward) => Some(EditorAction::DeleteBackward.into()),
            Some(KeyAction::DeleteForward) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && app.editor.is_empty() {
                    Some(AppAction::Quit.into())
                } else {
                    Some(EditorAction::DeleteForward.into())
                }
            }
            Some(KeyAction::DeleteWordBackward) => Some(EditorAction::DeleteBackward.into()),
            Some(KeyAction::DeleteWordForward) => Some(EditorAction::DeleteForward.into()),
            Some(KeyAction::DeleteToLineStart) => Some(EditorAction::DeleteBackward.into()),
            Some(KeyAction::DeleteToLineEnd) => Some(EditorAction::DeleteForward.into()),
            Some(KeyAction::Submit) => Some(EditorAction::Submit.into()),
            Some(KeyAction::Complete) => Some(EditorAction::AcceptSuggestion.into()),
            Some(KeyAction::CursorLeft | KeyAction::CursorWordLeft) => {
                Some(EditorAction::CursorLeft.into())
            }
            Some(KeyAction::CursorRight | KeyAction::CursorWordRight) => {
                Some(EditorAction::CursorRight.into())
            }
            Some(KeyAction::CursorLineStart) => Some(EditorAction::CursorLineStart.into()),
            Some(KeyAction::CursorLineEnd) => Some(EditorAction::CursorLineEnd.into()),
            Some(KeyAction::Cancel | KeyAction::Clear | KeyAction::Interrupt) => {
                Some(EditorAction::Cancel.into())
            }
            Some(KeyAction::TimelinePageUp) => Some(TimelineAction::ScrollUp(8).into()),
            Some(KeyAction::TimelinePageDown) => Some(TimelineAction::ScrollDown(8).into()),
            Some(KeyAction::SelectPrev | KeyAction::TimelineUp) => {
                Some(TimelineAction::ScrollUp(1).into())
            }
            Some(KeyAction::SelectNext | KeyAction::TimelineDown) => {
                Some(TimelineAction::ScrollDown(1).into())
            }
            Some(KeyAction::TimelineLatest) => Some(TimelineAction::JumpLatest.into()),
            Some(KeyAction::Models) => Some(ModelAction::RequestList.into()),
            None => {
                if let KeyCode::Char(ch) = key.code {
                    Some(EditorAction::InsertChar(ch).into())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
