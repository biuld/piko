use std::time::Instant;

use gpui::*;
use piko_chrome::components::notification::{
    NotificationPanelSpec, NotificationRowSpec, render_notification_panel, render_notification_row,
};

use super::model::{NotificationCenterState, NotificationId, relative_time};

pub fn render_notification_center<Remove, Clear>(
    state: &NotificationCenterState,
    viewport: Size<Pixels>,
    on_remove: Remove,
    on_clear: Clear,
) -> AnyElement
where
    Remove: Fn(NotificationId, &ClickEvent, &mut Window, &mut App) + Clone + 'static,
    Clear: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let now = Instant::now();
    let rows = state
        .records()
        .map(|record| {
            let id = record.id;
            let on_remove = on_remove.clone();
            render_notification_row(
                NotificationRowSpec {
                    id: ElementId::Name(format!("notification-{}", id.value()).into()),
                    remove_id: ElementId::Name(
                        format!("notification-remove-{}", id.value()).into(),
                    ),
                    severity: record.severity,
                    title: record.title.clone().into(),
                    message: record.message.clone().into(),
                    time: relative_time(record.created_at, now).into(),
                    remove_label: crate::t!("notifications.action.remove").into(),
                },
                move |event, window, cx| on_remove(id, event, window, cx),
            )
        })
        .collect();

    render_notification_panel(
        NotificationPanelSpec {
            title: crate::t!("notifications.title").into(),
            clear_label: crate::t!("notifications.action.clear_all").into(),
            empty_title: crate::t!("notifications.empty").into(),
            viewport,
        },
        rows,
        on_clear,
    )
}
