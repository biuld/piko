use super::*;

impl Timeline {
    pub fn new() -> Self {
        Self {
            components: VecDeque::new(),
            viewport: ScrollViewport::default(),
            thinking_visible: true,
            tool_calls: Vec::new(),
            hit_ids: HashMap::new(),
            thought_hit_ids: HashMap::new(),
            thought_starts: HashMap::new(),
            next_hit_id: 1,
            layout_epoch: 0,
            projection: piko_client_core::AgentTimeline::new(),
            next_local_id: 1,
            line_cache: std::cell::RefCell::new(super::line_cache::LineCache::default()),
            selection: std::cell::RefCell::new(super::selection::TimelineSelection::default()),
            projection_dirty: false,
            defer_projection_sync: false,
        }
    }

    pub fn begin_projection_batch(&mut self) {
        self.defer_projection_sync = true;
        self.projection.begin_batch();
    }

    pub fn end_projection_batch(&mut self) {
        self.projection.end_batch();
        self.defer_projection_sync = false;
        self.flush_projection();
    }

    fn mark_projection_applied(&mut self) {
        if self.defer_projection_sync {
            self.projection_dirty = true;
        } else {
            self.sync_projection();
        }
    }

    fn flush_projection(&mut self) {
        if self.projection_dirty {
            self.projection_dirty = false;
            self.sync_projection();
        }
    }

    pub fn push(&mut self, entry: TimelineEntry) {
        match entry {
            TimelineEntry::Tool(tool) => {
                let updated = self.upsert_tool(tool.clone());
                if !updated {
                    self.push_component(TimelineComponent::Tool(tool));
                }
            }
            TimelineEntry::Error(text) => self.push_error(text),
        }
    }

    pub fn apply_stream_item(
        &mut self,
        patch: &piko_protocol::StreamItemPatch,
    ) -> piko_client_core::ApplyOutcome {
        let outcome = self.projection.apply_stream_item(patch);
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome
    }

    pub fn apply_committed(&mut self, event: TranscriptCommittedEvent) -> bool {
        let outcome = self.projection.apply_committed_checked(
            event.message_id,
            event.transcript_seq,
            event.message,
            event.source_turn_id,
        );
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome != piko_client_core::ApplyOutcome::Inconsistent
    }

    pub fn apply_model_step_committed(
        &mut self,
        boundary: ModelStepBoundary,
    ) -> piko_client_core::ApplyOutcome {
        let outcome = self.projection.apply_model_step_committed(boundary);
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome
    }

    pub fn apply_session_entry(
        &mut self,
        entry: piko_protocol::SessionTreeEntry,
        branch_order: u64,
    ) -> piko_client_core::ApplyOutcome {
        let outcome = self.projection.apply_session_entry(entry, branch_order);
        if outcome == piko_client_core::ApplyOutcome::Applied {
            self.mark_projection_applied();
        }
        outcome
    }

    pub fn project_tool_started(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
    ) {
        self.projection
            .apply_tool_started(tool_call_id, tool_name, args, parent_message_id);
        self.mark_projection_applied();
    }

    pub fn finish_turn(&mut self, turn_id: &str, status: crate::app::ToolStatus) {
        let status = match status {
            crate::app::ToolStatus::Failed => piko_client_core::ToolStatus::Failed,
            crate::app::ToolStatus::Cancelled => piko_client_core::ToolStatus::Cancelled,
            _ => return,
        };
        self.projection.finish_turn(turn_id, status);
        self.mark_projection_applied();
    }

    pub fn push_error(&mut self, text: String) {
        self.push_anchored_error(text, None);
    }

    pub fn push_turn_error(&mut self, turn_id: &str, text: String) {
        self.push_anchored_error(text, Some(turn_id.to_string()));
    }

    fn push_anchored_error(&mut self, text: String, after_turn_id: Option<String>) {
        let anchored = after_turn_id.is_some();
        let id = self.local_id();
        self.push_component(TimelineComponent::Error(ErrorComponent {
            id,
            text,
            after_turn_id,
        }));
        if anchored {
            self.mark_projection_applied();
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.viewport.scroll_up(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.viewport.scroll_down(amount);
    }

    pub fn jump_latest(&mut self) {
        self.viewport.jump_latest();
    }

    pub(crate) fn start_selection(&mut self, point: SelectionPoint) {
        self.selection.get_mut().start(point);
    }

    pub(crate) fn update_selection(&mut self, point: SelectionPoint) {
        self.selection.get_mut().update(point);
    }

    pub(crate) fn finish_selection(&mut self, point: SelectionPoint) -> bool {
        self.selection.get_mut().finish(point)
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection.borrow().is_active()
    }

    pub(crate) fn selection_in_progress(&self) -> bool {
        self.selection.borrow().is_dragging()
    }

    pub(crate) fn selected_text(&self) -> Option<String> {
        self.selection.borrow().selected_text()
    }

    pub fn clear(&mut self) {
        self.components.clear();
        self.tool_calls.clear();
        self.hit_ids.clear();
        self.thought_hit_ids.clear();
        self.thought_starts.clear();
        // `next_hit_id` stays monotonic so ids are never reused after a clear.
        self.viewport.jump_latest();
        self.projection.clear();
        self.line_cache.borrow_mut().clear();
        self.selection.get_mut().clear();
        self.projection_dirty = false;
        self.defer_projection_sync = false;
        self.bump_layout_epoch();
    }

    /// Update or insert a presentation-only tool. Host-authored tools enter
    /// through the canonical client-core projection above.
    pub fn upsert_tool(&mut self, mut tool: ToolEntry) -> bool {
        tool.component_id = ComponentId::ToolCallId(tool.id.clone());
        self.intern_tool_id(&tool.id);
        if let Some(existing) = self.tool_calls.iter_mut().find(|t| t.id == tool.id) {
            tool.expanded = existing.expanded;
            *existing = tool.clone();
        } else {
            self.tool_calls.push(tool.clone());
        }
        if let Some(index) = self.components.iter().rposition(|component| {
            matches!(component, TimelineComponent::Tool(existing) if existing.id == tool.id)
        }) {
            self.components[index] = TimelineComponent::Tool(tool);
            self.bump_layout_epoch();
            return true;
        }
        self.bump_layout_epoch();
        false
    }

    fn sync_projection(&mut self) {
        use piko_client_core::TimelineItem as CoreItem;

        let expanded: HashMap<String, bool> = self
            .tool_calls
            .iter()
            .map(|tool| (tool.id.clone(), tool.expanded))
            .collect();
        let local_errors: Vec<TimelineComponent> = self
            .components
            .iter()
            .filter(|component| matches!(component, TimelineComponent::Error(_)))
            .cloned()
            .collect();
        let was_at_latest = self.viewport.is_at_latest();
        self.components.clear();
        self.tool_calls.clear();
        let now = std::time::Instant::now();
        let closed_thoughts: std::collections::HashSet<ThoughtKey> = self
            .projection
            .items()
            .iter()
            .filter_map(|item| {
                let CoreItem::RealtimeDraft(draft) = item else {
                    return None;
                };
                Some(
                    draft
                        .content_segments
                        .iter()
                        .filter(|segment| {
                            segment.kind == piko_client_core::RealtimeContentKind::Thinking
                                && !segment.text.is_empty()
                                && (draft.ended
                                    || draft.active_thinking_index != Some(segment.content_index))
                        })
                        .map(|segment| ThoughtKey {
                            message_id: draft.message_id.clone(),
                            segment_index: segment.content_index,
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect();

        // Intern hit ids for every projected tool before rebuilding components
        // (the projection borrow must end before mutating self).
        let tool_call_ids: Vec<String> = self
            .projection
            .items()
            .iter()
            .filter_map(|item| match item {
                CoreItem::Tool(tool) => Some(tool.tool_call_id.clone()),
                _ => None,
            })
            .collect();
        for tool_call_id in &tool_call_ids {
            self.intern_tool_id(tool_call_id);
        }

        // Upstream tool cards are projected interleaved with their committed
        // assistant message's text runs. Collect them here so the committed
        // message can place them at the right content position, and skip the
        // standalone copy in the item loop.
        let upstream_tools: HashMap<String, ToolEntry> = self
            .projection
            .items()
            .iter()
            .filter_map(|item| {
                let CoreItem::Tool(tool) = item else {
                    return None;
                };
                tool.upstream.as_ref()?;
                Some((
                    tool.tool_call_id.clone(),
                    super::projection::project_tool_item(tool, &expanded),
                ))
            })
            .collect();
        let mut emitted_upstream: std::collections::HashSet<String> = self
            .projection
            .items()
            .iter()
            .flat_map(|item| {
                let CoreItem::Committed(committed) = item else {
                    return Vec::new();
                };
                let piko_protocol::Message::Assistant { content, .. } = &committed.message else {
                    return Vec::new();
                };
                content
                    .iter()
                    .filter_map(|block| match block {
                        piko_protocol::ContentBlock::UpstreamToolActivity {
                            activity_id, ..
                        } => Some(activity_id.clone()),
                        piko_protocol::ContentBlock::UpstreamToolApproval {
                            approval_id, ..
                        } => Some(approval_id.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        // Live drafts: if an upstream tool captured a before-snapshot, the
        // single streaming message is split into  text-before → card → text-after.
        let mut draft_slices: HashMap<String, Vec<super::projection::DraftSlice>> = HashMap::new();
        for item in self.projection.items() {
            let CoreItem::Tool(tool) = item else {
                continue;
            };
            let (Some(upstream_split), Some(parent)) = (
                tool.upstream_split.as_ref(),
                tool.parent_message_id.as_ref(),
            ) else {
                continue;
            };
            draft_slices
                .entry(parent.clone())
                .or_default()
                .push(super::projection::DraftSlice {
                    tool: super::projection::project_tool_item(tool, &expanded),
                    text_before: upstream_split.before_text.chars().count(),
                    thinking_before: upstream_split.before_thinking.chars().count(),
                });
        }

        let mut last_component_by_turn = HashMap::new();
        for item in self.projection.items() {
            let source_turn_id = match item {
                CoreItem::Committed(committed) => Some(committed.source_turn_id.clone()),
                CoreItem::Tool(tool) => tool.source_turn_id.clone(),
                CoreItem::RealtimeDraft(_) | CoreItem::SessionEntry(_) => None,
            };
            let components: Vec<TimelineComponent> = match item {
                CoreItem::Committed(committed) => super::projection::components_from_message(
                    committed.message_id.clone(),
                    &committed.message,
                    &expanded,
                    &upstream_tools,
                ),
                CoreItem::RealtimeDraft(draft) => {
                    if let Some(mut slices) = draft_slices.get(&draft.message_id).cloned() {
                        slices.sort_by_key(|s| (s.text_before, s.thinking_before));
                        for slice in &slices {
                            emitted_upstream.insert(slice.tool.id.clone());
                        }
                        super::projection::components_from_draft(draft, &slices)
                    } else {
                        super::projection::components_from_draft(draft, &[])
                    }
                }
                CoreItem::Tool(tool) => {
                    // Upstream cards already emitted inside their committed
                    // assistant message; skip the standalone copy.
                    if tool.upstream.is_some() && emitted_upstream.contains(&tool.tool_call_id) {
                        Vec::new()
                    } else {
                        vec![TimelineComponent::Tool(
                            super::projection::project_tool_item(tool, &expanded),
                        )]
                    }
                }
                CoreItem::SessionEntry(entry) => {
                    super::projection::component_from_session_entry(&entry.entry)
                        .into_iter()
                        .collect()
                }
            };
            for component in components {
                let component = match component {
                    TimelineComponent::Thought(mut thought) => {
                        let key = thought.key.clone();
                        if closed_thoughts.contains(&key) {
                            let started = self.thought_starts.entry(key).or_insert(now);
                            thought.phase = ThoughtPhase::Completed {
                                duration_ms: Some(
                                    now.saturating_duration_since(*started)
                                        .as_millis()
                                        .try_into()
                                        .unwrap_or(u64::MAX),
                                ),
                            };
                        } else if matches!(thought.phase, ThoughtPhase::Streaming { .. }) {
                            let started = self.thought_starts.entry(key).or_insert(now);
                            thought.phase = ThoughtPhase::Streaming {
                                observed_at: *started,
                            };
                        }
                        TimelineComponent::Thought(thought)
                    }
                    other => other,
                };
                if let TimelineComponent::Tool(tool) = &component
                    && !self.tool_calls.iter().any(|t| t.id == tool.id)
                {
                    self.tool_calls.push(tool.clone());
                }
                self.components.push_back(component);
                if let Some(turn_id) = &source_turn_id {
                    last_component_by_turn.insert(turn_id.clone(), self.components.len() - 1);
                }
            }
        }
        let thought_keys: Vec<ThoughtKey> = self
            .components
            .iter()
            .filter_map(|component| match component {
                TimelineComponent::Thought(thought) => Some(thought.key.clone()),
                _ => None,
            })
            .collect();
        for key in thought_keys {
            self.intern_thought_key(key);
        }
        for error in local_errors {
            let insertion_index = match &error {
                TimelineComponent::Error(error) => error
                    .after_turn_id
                    .as_ref()
                    .and_then(|turn_id| last_component_by_turn.get(turn_id))
                    .map(|index| index + 1),
                _ => None,
            };
            if let Some(index) = insertion_index {
                self.components.insert(index, error);
                for last_index in last_component_by_turn.values_mut() {
                    if *last_index >= index {
                        *last_index += 1;
                    }
                }
            } else {
                self.components.push_back(error);
            }
        }
        while self.components.len() > MAX_COMPONENTS {
            self.components.pop_front();
        }
        self.hit_ids
            .retain(|tool_call_id, _| self.tool_calls.iter().any(|tool| &tool.id == tool_call_id));
        let thought_keys: std::collections::HashSet<ThoughtKey> = self
            .components
            .iter()
            .filter_map(|component| match component {
                TimelineComponent::Thought(thought) => Some(thought.key.clone()),
                _ => None,
            })
            .collect();
        self.thought_hit_ids
            .retain(|key, _| thought_keys.contains(key));
        self.thought_starts
            .retain(|key, _| thought_keys.contains(key));
        if was_at_latest {
            self.viewport.jump_latest();
        } else {
            self.viewport.mark_appended();
        }
        self.bump_layout_epoch();
    }
}
