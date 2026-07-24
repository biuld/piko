//! Pure enabled-item cursor movement.

use super::ContextMenuItem;

pub(crate) fn step(
    items: &[ContextMenuItem],
    current: Option<usize>,
    direction: isize,
) -> Option<usize> {
    let selectable = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.selectable().then_some(index))
        .collect::<Vec<_>>();
    if selectable.is_empty() {
        return None;
    }
    let position =
        current.and_then(|current| selectable.iter().position(|index| *index == current));
    Some(if direction < 0 {
        position
            .map(|position| selectable[(position + selectable.len() - 1) % selectable.len()])
            .unwrap_or_else(|| *selectable.last().expect("selectable is non-empty"))
    } else {
        position
            .map(|position| selectable[(position + 1) % selectable.len()])
            .unwrap_or(selectable[0])
    })
}

#[cfg(test)]
mod tests {
    use crate::components::menu::ContextMenuItem;

    use super::step;

    fn items() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::action("A", |_, _| {}),
            ContextMenuItem::separator(),
            ContextMenuItem::action("B", |_, _| {}).enabled(false),
            ContextMenuItem::action("C", |_, _| {}),
        ]
    }

    #[test]
    fn moves_wrap_and_skip_non_actions() {
        let items = items();
        assert_eq!(step(&items, None, 1), Some(0));
        assert_eq!(step(&items, None, -1), Some(3));
        assert_eq!(step(&items, Some(0), 1), Some(3));
        assert_eq!(step(&items, Some(3), 1), Some(0));
        assert_eq!(step(&items, Some(0), -1), Some(3));
    }
}
