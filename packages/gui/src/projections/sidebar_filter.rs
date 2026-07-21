//! Substring filter for session sidebar projection (window-local query).

use super::view_model::{SidebarGroup, SidebarViewModel};

/// Returns a copy of `vm` with rows/groups that do not match `query` removed.
pub fn apply_sidebar_search(vm: &SidebarViewModel, query: &str) -> SidebarViewModel {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return vm.clone();
    }

    let pinned: Vec<_> = vm
        .pinned
        .iter()
        .filter(|row| row_matches(&q, &row.label, &row.session_id, &row.cwd_hint, None))
        .cloned()
        .collect();

    let groups = vm
        .groups
        .iter()
        .filter_map(|group| {
            let rows: Vec<_> = group
                .rows
                .iter()
                .filter(|row| row_matches(&q, &row.label, &row.session_id, "", Some(&group.label)))
                .cloned()
                .collect();
            if rows.is_empty() {
                None
            } else {
                Some(SidebarGroup {
                    cwd: group.cwd.clone(),
                    label: group.label.clone(),
                    rows,
                })
            }
        })
        .collect();

    SidebarViewModel { pinned, groups }
}

fn row_matches(
    q: &str,
    label: &str,
    session_id: &str,
    cwd_hint: &str,
    group_label: Option<&str>,
) -> bool {
    label.to_lowercase().contains(q)
        || session_id.to_lowercase().contains(q)
        || (!cwd_hint.is_empty() && cwd_hint.to_lowercase().contains(q))
        || group_label.is_some_and(|g| g.to_lowercase().contains(q))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projections::{SessionRow, SessionRowKind};

    #[test]
    fn empty_query_is_identity() {
        let vm = SidebarViewModel {
            pinned: vec![SessionRow {
                session_id: "a".into(),
                label: "Alpha".into(),
                kind: SessionRowKind::Listed,
                message_count: 0,
                is_pinned: true,
                cwd_hint: "proj".into(),
            }],
            groups: vec![],
        };
        let out = apply_sidebar_search(&vm, "  ");
        assert_eq!(out.pinned.len(), 1);
    }

    #[test]
    fn filter_drops_non_matching_groups() {
        let vm = SidebarViewModel {
            pinned: vec![],
            groups: vec![
                SidebarGroup {
                    cwd: "/p/a".into(),
                    label: "alpha".into(),
                    rows: vec![SessionRow {
                        session_id: "1".into(),
                        label: "keep".into(),
                        kind: SessionRowKind::Listed,
                        message_count: 0,
                        is_pinned: false,
                        cwd_hint: String::new(),
                    }],
                },
                SidebarGroup {
                    cwd: "/p/b".into(),
                    label: "beta".into(),
                    rows: vec![SessionRow {
                        session_id: "2".into(),
                        label: "drop".into(),
                        kind: SessionRowKind::Listed,
                        message_count: 0,
                        is_pinned: false,
                        cwd_hint: String::new(),
                    }],
                },
            ],
        };
        let out = apply_sidebar_search(&vm, "keep");
        assert_eq!(out.groups.len(), 1);
        assert_eq!(out.groups[0].rows[0].session_id, "1");
    }
}
