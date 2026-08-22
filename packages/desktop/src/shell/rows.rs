use super::*;

pub(super) fn render_row(row: &timeline::TimelineRow, material: &WindowMaterialHost) -> AnyElement {
    let t = tokens();
    let m = metrics();
    let markdown = |source: &str| -> AnyElement {
        island::components::markdown::render_markdown(
            &island::components::markdown::parse_markdown(source),
        )
        .into_any_element()
    };
    let content = match row {
        timeline::TimelineRow::User { text, .. } => markdown(text),
        timeline::TimelineRow::Assistant { text, .. } => markdown(text),
        timeline::TimelineRow::Thinking { text, .. } => div()
            .border_l_2()
            .border_color(hairline(SurfaceRole::Chrome))
            .pl(m.space_sm)
            .child(
                island::theme::text(TextRole::Meta)
                    .text_color(t.muted_fg_rgba())
                    .child(text.clone()),
            )
            .into_any_element(),
        timeline::TimelineRow::Tool {
            name,
            detail,
            running,
            failed,
            ..
        } => {
            let accent = if *failed {
                t.role_accent(RoleAccent::Danger)
            } else if *running {
                t.role_accent(RoleAccent::Info)
            } else {
                t.muted_fg_rgba()
            };
            div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .px(m.space_sm)
                .py(px(4.))
                .rounded_sm()
                .border_1()
                .border_color(hairline(SurfaceRole::Chrome))
                .bg(fill(SurfaceRole::Content, *material))
                .child(
                    div().flex().items_center().gap(m.space_xs).child(
                        island::theme::text(TextRole::Label)
                            .text_color(accent)
                            .child(format!("⚙ {name}")),
                    ),
                )
                .when(!detail.is_empty(), |card| {
                    card.child(
                        island::theme::text(TextRole::Meta)
                            .text_color(t.muted_fg_rgba())
                            .child(detail.clone()),
                    )
                })
                .into_any_element()
        }
        timeline::TimelineRow::System { label, .. } => div()
            .flex()
            .justify_center()
            .child(
                island::theme::text(TextRole::Meta)
                    .text_color(t.muted_fg_rgba())
                    .child(label.clone()),
            )
            .into_any_element(),
    };
    div()
        .id(gpui::SharedString::from(row.id().to_string()))
        .child(content)
        .into_any_element()
}
