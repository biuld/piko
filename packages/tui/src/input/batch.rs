use crate::app::command::TimelineAction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    Up,
    Down,
}

/// Coalesces adjacent Timeline wheel actions without reordering direction
/// changes or crossing non-scroll actions.
#[derive(Default)]
pub struct TimelineScrollBatch {
    pending: Option<(Direction, usize)>,
}

impl TimelineScrollBatch {
    /// Queue one Timeline action. Non-wheel actions are returned unchanged
    /// after the caller flushes the pending wheel action.
    pub fn push(&mut self, action: TimelineAction) -> Option<TimelineAction> {
        let (direction, amount) = match action {
            TimelineAction::ScrollUp(amount) => (Direction::Up, amount),
            TimelineAction::ScrollDown(amount) => (Direction::Down, amount),
            other => return Some(other),
        };
        match self.pending {
            Some((current, total)) if current == direction => {
                self.pending = Some((current, total.saturating_add(amount)));
                None
            }
            Some(_) => {
                let flushed = self.take();
                self.pending = Some((direction, amount));
                flushed
            }
            None => {
                self.pending = Some((direction, amount));
                None
            }
        }
    }

    pub fn take(&mut self) -> Option<TimelineAction> {
        self.pending
            .take()
            .map(|(direction, amount)| match direction {
                Direction::Up => TimelineAction::ScrollUp(amount),
                Direction::Down => TimelineAction::ScrollDown(amount),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_wheel_steps_coalesce() {
        let mut batch = TimelineScrollBatch::default();
        assert!(batch.push(TimelineAction::ScrollUp(3)).is_none());
        assert!(batch.push(TimelineAction::ScrollUp(3)).is_none());
        assert!(matches!(batch.take(), Some(TimelineAction::ScrollUp(6))));
    }

    #[test]
    fn direction_change_flushes_in_order() {
        let mut batch = TimelineScrollBatch::default();
        assert!(batch.push(TimelineAction::ScrollUp(6)).is_none());
        assert!(matches!(
            batch.push(TimelineAction::ScrollDown(3)),
            Some(TimelineAction::ScrollUp(6))
        ));
        assert!(matches!(batch.take(), Some(TimelineAction::ScrollDown(3))));
    }
}
