use std::time::Instant;

use gpui::*;
use island::components::notification::{
    NotificationPanelSpec, NotificationRowSpec, render_notification_panel, render_notification_row,
};

use super::model::{AppNotificationCenter, relative_time};

pub fn render_notification_center<Remove, Clear>(
    center: &AppNotificationCenter,
    viewport: Size<Pixels>,
    on_remove: Remove,
    on_clear: Clear,
) -> AnyElement
where
    Remove: Fn(u64, &ClickEvent, &mut Window, &mut App) + Clone + 'static,
    Clear: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let now = Instant::now();
    let rows = center
        .records()
        .iter()
        .map(|record| {
            let row_id = record.row_id;
            let on_remove = on_remove.clone();
            render_notification_row(
                NotificationRowSpec {
                    id: ElementId::Name(format!("notification-{row_id}").into()),
                    remove_id: ElementId::Name(format!("notification-remove-{row_id}").into()),
                    severity: record.message.severity,
                    title: record.message.title.clone().into(),
                    message: record.message.message.clone().into(),
                    time: relative_time(record.message.created_at, now).into(),
                    remove_label: crate::t!("notifications.action.remove").into(),
                },
                move |event, window, cx| on_remove(row_id, event, window, cx),
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
