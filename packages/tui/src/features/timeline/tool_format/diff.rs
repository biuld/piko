//! IDE-style inline diff for timeline `edit` tool cards.
//!
//! Shows path, line-number gutter, red/green change rows, and collapsed
//! unchanged regions — not git-patch headers like `@@ edit 1 @@`.

use serde_json::Value;

use super::util::{MAX_BODY_LINES, str_field};

/// Structured diff body for paint (gutter + change colors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffView {
    pub path: String,
    /// e.g. `+2 −1`
    pub stats: String,
    pub rows: Vec<DiffRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRow {
    /// Unchanged context line.
    Context {
        old_no: usize,
        new_no: usize,
        text: String,
    },
    /// Removed line (old side only).
    Delete { old_no: usize, text: String },
    /// Added line (new side only).
    Insert { new_no: usize, text: String },
    /// Collapsed run of unchanged lines.
    Ellipsis { omitted: usize },
}

/// Build an IDE-style view for the `edit` tool from args and/or file-change details.
pub(super) fn present_edit_diff(
    args: Option<&Value>,
    result: Option<&Value>,
    details: Option<&Value>,
) -> Option<DiffView> {
    // Prefer durable full-file before/after — real line numbers + context.
    if let Some(change) = details
        .and_then(extract_file_change)
        .or_else(|| result.and_then(extract_file_change))
    {
        let path = str_field(change, "path").unwrap_or("file");
        let before = str_field(change, "before").unwrap_or("");
        let after = str_field(change, "after").unwrap_or("");
        return Some(file_contents_diff(path, before, after));
    }

    // While running (or without details), render each replace as a local hunk.
    let args = args?;
    let path = str_field(args, "path").unwrap_or("file");
    let edits = args.get("edits").and_then(Value::as_array)?;
    if edits.is_empty() {
        return None;
    }
    Some(edit_patches_diff(path, edits))
}

/// Extract `_pikoFileChange` from a details / result object when present.
pub(super) fn extract_file_change(value: &Value) -> Option<&Value> {
    if value.get("path").is_some()
        && (value.get("before").is_some() || value.get("after").is_some())
    {
        return Some(value);
    }
    value.get("_pikoFileChange")
}

impl DiffView {
    /// Plain-text projection for tests and simple consumers.
    #[allow(dead_code)]
    pub fn to_plain_lines(&self) -> Vec<String> {
        let mut lines = vec![self.path.clone()];
        if !self.stats.is_empty() {
            lines[0] = format!("{}  {}", self.path, self.stats);
        }
        let gutter_w = self.gutter_width();
        for row in &self.rows {
            lines.push(row.to_plain(gutter_w));
        }
        lines
    }

    /// Width of the single line-number gutter column.
    pub fn gutter_width(&self) -> usize {
        let mut max = 1usize;
        for row in &self.rows {
            if let Some(n) = row.line_no() {
                max = max.max(digits(n));
            }
        }
        max
    }
}

impl DiffRow {
    /// Single gutter number: old side for deletes, new side for inserts/context.
    fn line_no(&self) -> Option<usize> {
        match self {
            DiffRow::Context { new_no, .. } => Some(*new_no),
            DiffRow::Delete { old_no, .. } => Some(*old_no),
            DiffRow::Insert { new_no, .. } => Some(*new_no),
            DiffRow::Ellipsis { .. } => None,
        }
    }

    #[allow(dead_code)]
    fn to_plain(&self, gutter_w: usize) -> String {
        match self {
            DiffRow::Context { text, .. } => {
                let n = self.line_no().unwrap_or(0);
                format!("{n:>gutter_w$}   {text}")
            }
            DiffRow::Delete { text, .. } => {
                let n = self.line_no().unwrap_or(0);
                format!("{n:>gutter_w$} − {text}")
            }
            DiffRow::Insert { text, .. } => {
                let n = self.line_no().unwrap_or(0);
                format!("{n:>gutter_w$} + {text}")
            }
            DiffRow::Ellipsis { omitted } => {
                if *omitted == 0 {
                    format!("{:>gutter_w$} ···", "")
                } else {
                    format!("{:>gutter_w$} ··· {omitted} lines", "")
                }
            }
        }
    }
}

fn digits(n: usize) -> usize {
    n.max(1).to_string().len()
}

pub(super) fn file_contents_as_diff_view(path: &str, before: &str, after: &str) -> DiffView {
    file_contents_diff(path, before, after)
}

fn file_contents_diff(path: &str, before: &str, after: &str) -> DiffView {
    let old_lines: Vec<&str> = before.lines().collect();
    let new_lines: Vec<&str> = after.lines().collect();
    let ops = compute_ops(&old_lines, &new_lines);
    let rows = ops_to_ide_rows(&ops);
    let (added, removed) = count_changes(&rows);
    DiffView {
        path: path.to_string(),
        stats: format_stats(added, removed),
        rows: truncate_rows(rows),
    }
}

fn edit_patches_diff(path: &str, edits: &[Value]) -> DiffView {
    let mut rows = Vec::new();
    let mut old_no = 1usize;
    let mut new_no = 1usize;
    let mut added = 0usize;
    let mut removed = 0usize;

    for (i, edit) in edits.iter().enumerate() {
        if i > 0 {
            rows.push(DiffRow::Ellipsis { omitted: 0 }); // visual separator between patches
        }
        let old = str_field(edit, "oldText").unwrap_or("");
        let new = str_field(edit, "newText").unwrap_or("");
        let old_parts: Vec<&str> = if old.is_empty() {
            Vec::new()
        } else {
            old.lines().collect()
        };
        let new_parts: Vec<&str> = if new.is_empty() {
            Vec::new()
        } else {
            new.lines().collect()
        };

        // Local LCS within the patch so multi-line replaces look like IDE hunks.
        let ops = compute_ops(&old_parts, &new_parts);
        for op in ops {
            match op {
                DiffOp::Equal(text) => {
                    rows.push(DiffRow::Context {
                        old_no,
                        new_no,
                        text,
                    });
                    old_no += 1;
                    new_no += 1;
                }
                DiffOp::Delete(text) => {
                    rows.push(DiffRow::Delete { old_no, text });
                    old_no += 1;
                    removed += 1;
                }
                DiffOp::Insert(text) => {
                    rows.push(DiffRow::Insert { new_no, text });
                    new_no += 1;
                    added += 1;
                }
            }
        }
    }

    // Separator ellipsis with omitted=0 is just a blank break — filter to real ones
    // and use empty context gap instead.
    let rows: Vec<DiffRow> = rows
        .into_iter()
        .map(|row| match row {
            DiffRow::Ellipsis { omitted: 0 } => DiffRow::Ellipsis { omitted: 0 },
            other => other,
        })
        .collect();

    DiffView {
        path: path.to_string(),
        stats: format_stats(added, removed),
        rows: truncate_rows(rows),
    }
}

fn format_stats(added: usize, removed: usize) -> String {
    match (added, removed) {
        (0, 0) => String::new(),
        (a, 0) => format!("+{a}"),
        (0, r) => format!("−{r}"),
        (a, r) => format!("+{a} −{r}"),
    }
}

fn count_changes(rows: &[DiffRow]) -> (usize, usize) {
    let mut added = 0usize;
    let mut removed = 0usize;
    for row in rows {
        match row {
            DiffRow::Insert { .. } => added += 1,
            DiffRow::Delete { .. } => removed += 1,
            _ => {}
        }
    }
    (added, removed)
}

#[derive(Clone)]
enum DiffOp {
    Equal(String),
    Delete(String),
    Insert(String),
}

fn compute_ops(old_lines: &[&str], new_lines: &[&str]) -> Vec<DiffOp> {
    const LCS_LIMIT: usize = 400;
    if old_lines.is_empty() && new_lines.is_empty() {
        return Vec::new();
    }
    if old_lines.len() > LCS_LIMIT || new_lines.len() > LCS_LIMIT {
        let mut ops = Vec::new();
        for line in old_lines {
            ops.push(DiffOp::Delete((*line).to_string()));
        }
        for line in new_lines {
            ops.push(DiffOp::Insert((*line).to_string()));
        }
        return ops;
    }

    let table = lcs_table(old_lines, new_lines);
    let mut ops = Vec::new();
    emit_ops(
        &table,
        old_lines,
        new_lines,
        old_lines.len(),
        new_lines.len(),
        &mut ops,
    );
    ops
}

/// Convert flat ops into IDE rows with context windows and collapsed equals.
fn ops_to_ide_rows(ops: &[DiffOp]) -> Vec<DiffRow> {
    // Annotate ops with line numbers first.
    let mut numbered: Vec<(DiffOp, usize, usize)> = Vec::new();
    let mut old_no = 1usize;
    let mut new_no = 1usize;
    for op in ops {
        match op {
            DiffOp::Equal(text) => {
                numbered.push((DiffOp::Equal(text.clone()), old_no, new_no));
                old_no += 1;
                new_no += 1;
            }
            DiffOp::Delete(text) => {
                numbered.push((DiffOp::Delete(text.clone()), old_no, 0));
                old_no += 1;
            }
            DiffOp::Insert(text) => {
                numbered.push((DiffOp::Insert(text.clone()), 0, new_no));
                new_no += 1;
            }
        }
    }

    if numbered.is_empty() {
        return Vec::new();
    }

    // Mark indices that are changes or within CONTEXT of a change.
    const CONTEXT: usize = 3;
    let mut keep = vec![false; numbered.len()];
    for (i, (op, _, _)) in numbered.iter().enumerate() {
        if !matches!(op, DiffOp::Equal(_)) {
            let start = i.saturating_sub(CONTEXT);
            let end = (i + CONTEXT + 1).min(numbered.len());
            for slot in keep.iter_mut().take(end).skip(start) {
                *slot = true;
            }
        }
    }
    // If the file is tiny or all equal, show everything (or a single empty note).
    if keep.iter().all(|k| !k) {
        // No changes — show a short preview of the file.
        return numbered
            .into_iter()
            .take(6)
            .map(|(op, o, n)| match op {
                DiffOp::Equal(text) => DiffRow::Context {
                    old_no: o,
                    new_no: n,
                    text,
                },
                DiffOp::Delete(text) => DiffRow::Delete { old_no: o, text },
                DiffOp::Insert(text) => DiffRow::Insert { new_no: n, text },
            })
            .collect();
    }

    let mut rows = Vec::new();
    let mut i = 0;
    while i < numbered.len() {
        if !keep[i] {
            let start = i;
            while i < numbered.len() && !keep[i] {
                i += 1;
            }
            let omitted = i - start;
            if omitted > 0 {
                rows.push(DiffRow::Ellipsis { omitted });
            }
            continue;
        }
        let (op, o, n) = &numbered[i];
        rows.push(match op {
            DiffOp::Equal(text) => DiffRow::Context {
                old_no: *o,
                new_no: *n,
                text: text.clone(),
            },
            DiffOp::Delete(text) => DiffRow::Delete {
                old_no: *o,
                text: text.clone(),
            },
            DiffOp::Insert(text) => DiffRow::Insert {
                new_no: *n,
                text: text.clone(),
            },
        });
        i += 1;
    }
    rows
}

fn lcs_table(a: &[&str], b: &[&str]) -> Vec<Vec<u16>> {
    let mut table = vec![vec![0u16; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            if a[i - 1] == b[j - 1] {
                table[i][j] = table[i - 1][j - 1].saturating_add(1);
            } else {
                table[i][j] = table[i - 1][j].max(table[i][j - 1]);
            }
        }
    }
    table
}

fn emit_ops(
    table: &[Vec<u16>],
    a: &[&str],
    b: &[&str],
    i: usize,
    j: usize,
    out: &mut Vec<DiffOp>,
) {
    if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
        emit_ops(table, a, b, i - 1, j - 1, out);
        out.push(DiffOp::Equal(a[i - 1].to_string()));
    } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
        emit_ops(table, a, b, i, j - 1, out);
        out.push(DiffOp::Insert(b[j - 1].to_string()));
    } else if i > 0 {
        emit_ops(table, a, b, i - 1, j, out);
        out.push(DiffOp::Delete(a[i - 1].to_string()));
    }
}

fn truncate_rows(mut rows: Vec<DiffRow>) -> Vec<DiffRow> {
    const MAX: usize = MAX_BODY_LINES + 12;
    if rows.len() <= MAX {
        return rows;
    }
    let omitted = rows.len() - MAX;
    rows.truncate(MAX);
    rows.push(DiffRow::Ellipsis { omitted });
    rows
}
