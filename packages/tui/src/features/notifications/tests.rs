use super::*;

#[test]
fn active_attention_precedes_transient_info_in_the_row() {
    let mut center = NotificationCenter::default();
    center.push(NotificationLevel::Warning, "keep me");
    for index in 0..20 {
        center.push(NotificationLevel::Info, format!("info {index}"));
    }

    assert_eq!(center.items.len(), 21);
    assert_eq!(
        center
            .row_visible_for(Instant::now(), None, None)
            .unwrap()
            .message,
        "keep me"
    );
}

#[test]
fn subject_resolution_and_scoping_are_stable() {
    let mut center = NotificationCenter::default();
    let subject = NoticeSubject::Approval("approval-1".into());
    center.push_with(
        NoticeScope::Session("session-1".into()),
        NotificationLevel::Warning,
        NoticePolicy::UntilResolved(subject.clone()),
        "approve",
    );

    assert!(
        center
            .row_visible_for(Instant::now(), Some("session-2"), None)
            .is_none()
    );
    assert!(
        center
            .row_visible_for(Instant::now(), Some("session-1"), None)
            .is_some()
    );
    center.resolve(&subject);
    assert!(
        center
            .row_visible_for(Instant::now(), Some("session-1"), None)
            .is_none()
    );
    assert_eq!(center.items.len(), 1);
    assert_eq!(center.items[0].status, NoticeStatus::Resolved);
}

#[test]
fn modal_defaults_to_current_and_can_show_all_sessions() {
    let mut center = NotificationCenter::default();
    center.push(NotificationLevel::Info, "global");
    center.push_with(
        NoticeScope::Session("session-1".into()),
        NotificationLevel::Warning,
        NoticePolicy::Dismissible,
        "current",
    );
    center.push_with(
        NoticeScope::Session("session-2".into()),
        NotificationLevel::Error,
        NoticePolicy::Dismissible,
        "other",
    );

    center.open_modal();
    assert_eq!(center.modal_items(Some("session-1")).len(), 2);
    center.set_view_scope(NotificationViewScope::All);
    assert_eq!(center.modal_items(Some("session-1")).len(), 3);
}

#[test]
fn mode_affix_pointer_switches_to_all_sessions() {
    let mut center = NotificationCenter::default();
    center.push_with(
        NoticeScope::Session("session-2".into()),
        NotificationLevel::Warning,
        NoticePolicy::Dismissible,
        "other",
    );
    center.open_modal();

    let actions = center.pointer_event(
        ComponentHit {
            element: Some(HitId::Mode(1)),
            rect: ratatui::layout::Rect::new(0, 0, 10, 1),
            x: 1,
            y: 0,
        },
        PointerGesture::Activate,
    );

    assert!(actions.is_empty());
    assert_eq!(center.view_scope, NotificationViewScope::All);
    assert_eq!(center.modal_items(Some("session-1")).len(), 1);
}

#[test]
fn dismiss_hides_only_the_visible_notice_and_keeps_history() {
    let mut center = NotificationCenter::default();
    center.push(NotificationLevel::Warning, "first");
    center.push(NotificationLevel::Error, "second");

    center.dismiss_visible(Instant::now(), None, None);

    assert_eq!(center.items.len(), 2);
    assert_eq!(center.items.back().unwrap().status, NoticeStatus::Dismissed);
    assert_eq!(
        center
            .row_visible_for(Instant::now(), None, None)
            .unwrap()
            .message,
        "first"
    );
}

#[test]
fn elapsed_info_leaves_the_row_but_remains_in_the_modal_queue() {
    let mut center = NotificationCenter::default();
    let now = Instant::now();
    center.push_with(
        NoticeScope::Global,
        NotificationLevel::Info,
        NoticePolicy::Transient { visible_until: now },
        "done",
    );

    assert!(center.row_visible_for(now, None, None).is_none());
    assert_eq!(center.items.len(), 1);
    assert_eq!(center.modal_items(None)[0].message, "done");
}

#[test]
fn snapshot_restore_reactivates_history_without_appending_a_duplicate() {
    let mut center = NotificationCenter::default();
    let subject = NoticeSubject::Approval("approval-1".into());
    let policy = NoticePolicy::UntilResolved(subject.clone());
    center.push_with(
        NoticeScope::Session("session-1".into()),
        NotificationLevel::Warning,
        policy.clone(),
        "approve",
    );
    center.clear_state_derived_for_session("session-1");

    center.restore_with(
        NoticeScope::Session("session-1".into()),
        NotificationLevel::Warning,
        policy,
        "approve",
    );

    assert_eq!(center.items.len(), 1);
    assert_eq!(center.items[0].status, NoticeStatus::Active);
}

#[test]
fn attention_history_is_not_capacity_evicted() {
    let mut center = NotificationCenter::default();
    for index in 0..40 {
        center.push(NotificationLevel::Warning, format!("warning {index}"));
    }

    assert_eq!(center.items.len(), 40);
}

#[test]
fn copied_feedback_is_scoped_to_one_notice_and_expires() {
    let mut center = NotificationCenter::default();
    let first = center.push(NotificationLevel::Info, "first");
    let second = center.push(NotificationLevel::Info, "second");
    let now = Instant::now();

    center.mark_copied(first, now);

    assert!(center.is_copied(first, now));
    assert!(!center.is_copied(second, now));
    assert!(!center.is_copied(first, now + COPY_FEEDBACK_TTL));
}
