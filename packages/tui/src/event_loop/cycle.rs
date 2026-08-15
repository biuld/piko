use std::time::Duration;

/// Fixed budgets for one serialized TUI cycle.
///
/// A cycle is the unit of work: input, then host, then tick, then at most
/// one paint. Neither ingress path may drain an unbounded queue.
#[derive(Clone, Copy, Debug)]
pub struct CycleBudget {
    pub max_host_lines: usize,
    pub max_input_events: usize,
    pub input_time: Duration,
    pub host_paint_interval: Duration,
    pub tick_interval: Duration,
    pub idle_wait: Duration,
}

impl CycleBudget {
    pub const fn standard() -> Self {
        Self {
            max_host_lines: 64,
            max_input_events: 64,
            input_time: Duration::from_millis(8),
            host_paint_interval: Duration::from_millis(33),
            tick_interval: Duration::from_millis(80),
            idle_wait: Duration::from_millis(50),
        }
    }
}

/// Work that landed in the current cycle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CycleWork {
    pub input: bool,
    pub host: bool,
    pub tick: bool,
}

/// Paint at most once per cycle. Input and ticks always paint. Host-only
/// stream updates wait for the host-paint interval so tokens cannot run
/// the loop at token rate.
pub fn should_paint(work: CycleWork, host_paint_due: bool) -> bool {
    work.input || work.tick || (work.host && host_paint_due)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_paints_even_when_host_interval_has_not_elapsed() {
        assert!(should_paint(
            CycleWork {
                input: true,
                host: true,
                tick: false
            },
            false
        ));
    }

    #[test]
    fn host_alone_waits_for_the_paint_interval() {
        assert!(!should_paint(
            CycleWork {
                input: false,
                host: true,
                tick: false
            },
            false
        ));
        assert!(should_paint(
            CycleWork {
                input: false,
                host: true,
                tick: false
            },
            true
        ));
    }

    #[test]
    fn tick_paints_without_input_or_host() {
        assert!(should_paint(
            CycleWork {
                input: false,
                host: false,
                tick: true
            },
            false
        ));
    }

    #[test]
    fn idle_cycle_does_not_paint() {
        assert!(!should_paint(CycleWork::default(), true));
    }
}
