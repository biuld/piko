use super::tree_row_prefix;
use crate::features::tree::visible::{ConnectorKind, Gutter, TreeRow};

#[test]
fn tree_row_prefix_places_connector_at_depth_position() {
    let row = TreeRow {
        entry_id: "branch".into(),
        depth: 2,
        connector: ConnectorKind::Corner,
        gutters: Vec::new(),
        is_active_path: false,
        is_folded: false,
        label: None,
        text_preview: String::new(),
        role_preview: String::new(),
    };

    assert_eq!(tree_row_prefix(&row), "   └─ ");
}

#[test]
fn tree_row_prefix_preserves_vertical_gutters() {
    let row = TreeRow {
        entry_id: "descendant".into(),
        depth: 3,
        connector: ConnectorKind::Branch,
        gutters: vec![Gutter {
            position: 0,
            kind: ConnectorKind::Vertical,
        }],
        is_active_path: false,
        is_folded: false,
        label: None,
        text_preview: String::new(),
        role_preview: String::new(),
    };

    assert_eq!(tree_row_prefix(&row), "│     ├─ ");
}
