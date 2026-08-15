use crate::{
    app::{
        AppState, HitId, SurfaceId,
        command::{Action, PointerAction, PointerTarget, SurfaceAction},
        effect::Effect,
    },
    navigation::{OutsideClickPolicy, Region},
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

impl AppState {
    pub(super) fn dispatch_pointer_action(&mut self, action: PointerAction) -> Vec<Effect> {
        let actions = self.reduce_pointer_action(action);
        let mut effects = Vec::new();
        for action in actions {
            effects.extend(self.dispatch(action));
        }
        effects
    }

    /// Pointer reducer shared by production dispatch and focused adapter tests.
    /// All pointer-owned state changes happen inside this reducer boundary.
    pub(crate) fn reduce_pointer_action(&mut self, action: PointerAction) -> Vec<Action> {
        match action {
            PointerAction::LeftDown(target) => {
                self.pointer_left_down = true;
                self.reduce_pointer_target(target, PointerGesture::Activate)
            }
            PointerAction::LeftUp(target) => {
                if std::mem::take(&mut self.pointer_left_down) {
                    Vec::new()
                } else {
                    self.reduce_pointer_target(target, PointerGesture::Activate)
                }
            }
            PointerAction::Move(hovered) => {
                self.hovered = hovered;
                Vec::new()
            }
            PointerAction::Gesture { target, gesture } => {
                self.reduce_pointer_target(target, gesture)
            }
        }
    }

    fn reduce_pointer_target(
        &mut self,
        target: PointerTarget,
        gesture: PointerGesture,
    ) -> Vec<Action> {
        match target {
            PointerTarget::None => Vec::new(),
            PointerTarget::OutsideModal(surface) => match (gesture, surface.outside_click_policy())
            {
                (PointerGesture::Activate, OutsideClickPolicy::Dismiss) => {
                    vec![SurfaceAction::Close.into()]
                }
                _ => Vec::new(),
            },
            PointerTarget::Component { region, hit } => {
                self.reduce_component_pointer(region, hit, gesture)
            }
        }
    }

    fn reduce_component_pointer(
        &mut self,
        region: Region,
        component_hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<Action> {
        match region {
            Region::Surface(SurfaceId::Approval) => {
                self.approvals.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::ToolInteraction) => {
                self.interactions.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Agents) => {
                self.agent_panel.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Sessions) => {
                self.sessions.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Models) => self.models.pointer_event(component_hit, gesture),
            Region::Surface(SurfaceId::Thinking) => {
                self.thinking.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Settings) => {
                self.settings.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::AuthSelector) => {
                self.auth_selector.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Mcp) => self.mcp.pointer_event(component_hit, gesture),
            Region::Surface(SurfaceId::Processes) => {
                self.processes.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Diagnostics) => {
                self.diagnostics.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Notifications) => {
                self.notifications.pointer_event(component_hit, gesture)
            }
            Region::Surface(SurfaceId::Tree) => self.tree.pointer_event(component_hit, gesture),
            Region::Surface(SurfaceId::SummaryPrompt) => {
                if gesture != PointerGesture::Activate {
                    return Vec::new();
                }
                let Some(workflow) = self.summary_prompt.as_mut() else {
                    return Vec::new();
                };
                match component_hit.element {
                    Some(HitId::Choice { choice, .. }) => {
                        workflow.select_choice(choice);
                        vec![SurfaceAction::Confirm.into()]
                    }
                    Some(HitId::Tab(step)) => {
                        workflow.goto_step(step);
                        Vec::new()
                    }
                    Some(HitId::Submit) => vec![SurfaceAction::Confirm.into()],
                    Some(HitId::TextInput) => {
                        workflow.move_active_input_to_column(component_hit.local_x());
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            Region::Guidance => self.notifications.pointer_event(component_hit, gesture),
            Region::Todos
                if gesture == PointerGesture::Activate
                    && component_hit.element == Some(HitId::TodosToggle) =>
            {
                self.todo_lists.toggle_collapsed();
                Vec::new()
            }
            Region::DockBoundary | Region::Todos => Vec::new(),
            Region::Suggest => self
                .editor
                .auto_complete
                .pointer_event(component_hit, gesture),
            Region::Composer => self.editor.pointer_event(component_hit, gesture),
            Region::Stream => self.timeline_mut().pointer_event(component_hit, gesture),
            _ => Vec::new(),
        }
    }
}
