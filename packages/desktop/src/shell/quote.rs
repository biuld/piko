//! Quote selection into the Composer draft (F-45). No host intent.

use super::*;

pub fn quote_markdown(selection: &str) -> String {
    let lines: Vec<&str> = if selection.is_empty() {
        vec![""]
    } else {
        selection.split('\n').collect()
    };
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.is_empty() {
            out.push('>');
        } else {
            out.push_str("> ");
            out.push_str(line);
        }
    }
    out.push_str("\n\n");
    out
}

pub fn insert_quote(draft: &str, selection: &str) -> String {
    let q = quote_markdown(selection);
    if draft.trim().is_empty() {
        q
    } else if draft.ends_with("\n\n") {
        format!("{draft}{q}")
    } else if draft.ends_with('\n') {
        format!("{draft}\n{q}")
    } else {
        format!("{draft}\n\n{q}")
    }
}

impl Shell {
    pub(super) fn quote_into_composer(
        &mut self,
        selection: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.composer_input.read(cx).value().to_string();
        let next = insert_quote(&current, &selection);
        self.composer_input.update(cx, |input, cx| {
            input.set_value(next.clone(), window, cx);
        });
        self.drafts.insert(self.draft_key.clone(), next);
        self.set_focus_owner(FocusOwner::Composer, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_keeps_trailing_empty_line() {
        assert_eq!(quote_markdown("hello\nworld"), "> hello\n> world\n\n");
        assert_eq!(quote_markdown("hello\n"), "> hello\n>\n\n");
    }

    #[test]
    fn insert_appends_with_blank_line() {
        assert_eq!(insert_quote("", "hi"), "> hi\n\n");
        assert_eq!(insert_quote("draft", "hi"), "draft\n\n> hi\n\n");
    }
}
