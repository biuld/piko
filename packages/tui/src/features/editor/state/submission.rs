use super::*;

impl Editor {
    pub fn take_submission(&mut self) -> Option<EditorSubmission> {
        let content = self.structured_content()?;
        let display_text = self.expanded_display_text().trim().to_string();
        self.push_history(display_text.clone());
        self.clear();
        Some(EditorSubmission {
            content,
            display_text,
        })
    }

    pub fn snapshot_draft(&self) -> EditorDraft {
        EditorDraft {
            text: self.text.clone(),
            cursor: self.cursor,
            references: self.references.clone(),
        }
    }

    pub fn restore_draft(&mut self, draft: EditorDraft) {
        self.text = draft.text;
        self.cursor = draft.cursor.min(self.text.len());
        self.references = draft.references;
        self.history_index = None;
        self.draft_before_history = None;
    }

    pub fn insert_image(&mut self, filename: &str, data: String, mime_type: String) {
        self.exit_history_browse();
        let id = self.next_reference_id;
        self.next_reference_id += 1;
        let placeholder = format!("[Image #{id}: {filename}]");
        self.references.push(ReferenceBlock {
            placeholder: placeholder.clone(),
            payload: ReferencePayload::Image { data, mime_type },
        });
        self.insert_str(&placeholder);
    }

    pub fn restore_content(&mut self, content: &piko_protocol::MessageContent) {
        self.clear();
        match content {
            piko_protocol::MessageContent::String(text) => self.insert_str(text),
            piko_protocol::MessageContent::Blocks(blocks) => {
                for block in blocks {
                    match block {
                        piko_protocol::ContentBlock::Text { text } => self.insert_str(text),
                        piko_protocol::ContentBlock::Image { data, mime_type } => {
                            self.insert_image("restored.png", data.clone(), mime_type.clone())
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn expanded_display_text(&self) -> String {
        let mut text = self.text.clone();
        for reference in &self.references {
            if let ReferencePayload::Text(content) = &reference.payload {
                text = text.replace(&reference.placeholder, content);
            }
        }
        text
    }

    fn structured_content(&self) -> Option<piko_protocol::MessageContent> {
        use piko_protocol::{ContentBlock, MessageContent};

        let mut blocks = Vec::new();
        let mut pending_text = String::new();
        let mut cursor = 0usize;
        let mut has_image = false;
        while cursor < self.text.len() {
            let Some((offset, reference)) = self
                .references
                .iter()
                .filter_map(|reference| {
                    self.text[cursor..]
                        .find(&reference.placeholder)
                        .map(|offset| (offset, reference))
                })
                .min_by_key(|(offset, _)| *offset)
            else {
                pending_text.push_str(&self.text[cursor..]);
                break;
            };
            let start = cursor + offset;
            pending_text.push_str(&self.text[cursor..start]);
            match &reference.payload {
                ReferencePayload::Text(text) => pending_text.push_str(text),
                ReferencePayload::Image { data, mime_type } => {
                    if !pending_text.is_empty() {
                        blocks.push(ContentBlock::Text {
                            text: std::mem::take(&mut pending_text),
                        });
                    }
                    blocks.push(ContentBlock::Image {
                        data: data.clone(),
                        mime_type: mime_type.clone(),
                    });
                    has_image = true;
                }
            }
            cursor = start + reference.placeholder.len();
        }
        if !pending_text.is_empty() {
            blocks.push(ContentBlock::Text { text: pending_text });
        }
        trim_boundary_text(&mut blocks);
        if has_image {
            Some(MessageContent::Blocks(blocks))
        } else {
            let text = blocks
                .into_iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text),
                    _ => None,
                })
                .collect::<String>();
            (!text.is_empty()).then_some(MessageContent::String(text))
        }
    }
}

fn trim_boundary_text(blocks: &mut Vec<piko_protocol::ContentBlock>) {
    if let Some(piko_protocol::ContentBlock::Text { text }) = blocks
        .iter_mut()
        .find(|block| matches!(block, piko_protocol::ContentBlock::Text { .. }))
    {
        *text = text.trim_start().to_string();
    }
    if let Some(piko_protocol::ContentBlock::Text { text }) = blocks
        .iter_mut()
        .rev()
        .find(|block| matches!(block, piko_protocol::ContentBlock::Text { .. }))
    {
        *text = text.trim_end().to_string();
    }
    blocks.retain(
        |block| !matches!(block, piko_protocol::ContentBlock::Text { text } if text.is_empty()),
    );
}
