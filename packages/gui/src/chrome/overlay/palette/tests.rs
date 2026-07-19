use piko_protocol::{HostCommandDescriptor, HostCommandGroup, HostCommandInvoke};

use super::{CommandPalette, LocalCommandId, PaletteRowAction};

fn host_item(id: &str, invoke: HostCommandInvoke) -> HostCommandDescriptor {
    HostCommandDescriptor {
        id: id.to_string(),
        title: id.to_string(),
        detail: "detail".into(),
        invoke,
        group: Some(HostCommandGroup::Session),
    }
}

#[test]
fn root_marks_model_set_as_submenu() {
    let catalog = vec![host_item("model.set", HostCommandInvoke::Immediate)];
    let frame = CommandPalette::root_frame(&catalog);
    assert!(matches!(
        frame.rows[0].action,
        PaletteRowAction::EnterModels
    ));
    assert!(frame.rows[0].enabled);
}

#[test]
fn root_marks_thinking_set_as_submenu() {
    let catalog = vec![host_item("thinking.set", HostCommandInvoke::Immediate)];
    let frame = CommandPalette::root_frame(&catalog);
    assert!(matches!(
        frame.rows[0].action,
        PaletteRowAction::EnterThinking
    ));
    assert!(frame.rows[0].enabled);
}

#[test]
fn root_enables_session_new_and_disables_unhandled_host_ids() {
    let catalog = vec![
        host_item("session.new", HostCommandInvoke::Immediate),
        host_item("session.rename", HostCommandInvoke::Args { schema: vec![] }),
        host_item("session.delete", HostCommandInvoke::Confirm),
    ];
    let frame = CommandPalette::root_frame(&catalog);
    assert!(frame.rows[0].enabled);
    assert!(matches!(
        &frame.rows[0].action,
        PaletteRowAction::Host(id) if id.as_str() == "session.new"
    ));
    assert!(!frame.rows[1].enabled);
    assert!(!frame.rows[2].enabled);
}

#[test]
fn root_appends_local_commands_after_host_catalog() {
    let frame = CommandPalette::root_frame(&[]);
    assert!(
        frame
            .rows
            .iter()
            .any(|row| matches!(row.action, PaletteRowAction::Local(LocalCommandId::Quit)))
    );
    assert!(frame.rows.iter().any(|row| matches!(
        row.action,
        PaletteRowAction::Local(LocalCommandId::FocusSessions)
    )));
}
