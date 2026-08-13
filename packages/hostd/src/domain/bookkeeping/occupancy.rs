//! Pure bookkeeping occupancy values.

/// Conservative occupancy of a projected session-tree slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextUsageEstimate {
    pub tokens: u64,
    pub usage_tokens: u64,
    pub trailing_tokens: u64,
    pub last_usage_index: Option<usize>,
}

impl ContextUsageEstimate {
    pub fn from_tokens(tokens: u64) -> Self {
        Self {
            tokens,
            usage_tokens: 0,
            trailing_tokens: tokens,
            last_usage_index: None,
        }
    }
}

/// Occupancy plus the last provider fill and catalog window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextOccupancy {
    pub estimated_tokens: u64,
    pub last_provider_fill: u64,
    pub window: Option<u64>,
}

impl ContextOccupancy {
    pub fn remaining(&self) -> Option<u64> {
        self.window
            .map(|window| window.saturating_sub(self.estimated_tokens))
    }
}

pub fn occupancy(
    estimated_tokens: u64,
    window: Option<u64>,
    last_usage: Option<&piko_protocol::messages::Usage>,
) -> ContextOccupancy {
    ContextOccupancy {
        estimated_tokens,
        last_provider_fill: last_usage
            .map(piko_protocol::messages::Usage::context_fill)
            .unwrap_or(0),
        window: window.filter(|value| *value > 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occupancy_keeps_provider_fill_separate_from_estimate() {
        let mut usage = piko_protocol::messages::Usage::empty();
        usage.input = 40;
        usage.cache_read = 10;
        let snapshot = occupancy(18, Some(128_000), Some(&usage));
        assert_eq!(snapshot.estimated_tokens, 18);
        assert_eq!(snapshot.last_provider_fill, 50);
        assert_eq!(snapshot.remaining(), Some(127_982));
    }
}
