//! Pure helpers for keyboard-driven flat lists (tree rows, settings nav, …).

/// Move a list cursor by `delta`, wrapping at ends. Empty list → `None`.
pub fn step_list_index(len: usize, current: Option<usize>, delta: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let cur = current.unwrap_or(0) as isize;
    let n = len as isize;
    let next = (cur + delta).rem_euclid(n) as usize;
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_wraps() {
        assert_eq!(step_list_index(3, Some(0), 1), Some(1));
        assert_eq!(step_list_index(3, Some(2), 1), Some(0));
        assert_eq!(step_list_index(3, Some(0), -1), Some(2));
        assert_eq!(step_list_index(0, None, 1), None);
        assert_eq!(step_list_index(5, None, 0), Some(0));
    }
}
