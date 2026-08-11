//! Context fill helpers for F-22 / D-34 usage projection.

use piko_protocol::messages::{Usage, UsageCost, UsageCostBasis, UsageCostEntry};

/// Prompt-side tokens used as an approximate context-window fill.
///
/// Matches TUI BottomBar (`input + cache_read`) until a host live estimate lands.
pub fn context_fill_from_usage(usage: &Usage) -> u64 {
    usage.context_fill()
}

/// True when the usage payload has any token or cost signal worth projecting.
pub fn usage_has_signal(usage: &Usage) -> bool {
    usage.total_tokens > 0
        || !usage.cost.entries.is_empty()
        || usage.input > 0
        || usage.output > 0
        || usage.cache_read > 0
        || usage.cache_write > 0
}

/// Compact token counts for status chrome (`1.5k`, `200k`, `1.2M`).
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if n.is_multiple_of(1_000_000) {
            format!("{}M", n / 1_000_000)
        } else {
            format!("{m:.1}M")
        }
    } else if n >= 1000 {
        if n.is_multiple_of(1000) {
            format!("{}k", n / 1000)
        } else {
            format!("{:.1}k", n as f64 / 1000.0)
        }
    } else {
        n.to_string()
    }
}

/// Human-readable context fill: `12.2k/200k`, partial `12.2k/—`, or `—/—`.
pub fn format_context(used: Option<u64>, size: Option<u64>) -> String {
    match (used, size) {
        (None, None) => "—/—".to_string(),
        (Some(used), None) => format!("{}/—", format_tokens(used)),
        (None, Some(size)) => format!("—/{}", format_tokens(size)),
        (Some(used), Some(size)) => {
            format!("{}/{}", format_tokens(used), format_tokens(size))
        }
    }
}

/// Format a native-currency cost ledger for compact client chrome.
pub fn format_cost(cost: &UsageCost) -> String {
    cost.entries
        .iter()
        .map(format_cost_entry)
        .collect::<Vec<_>>()
        .join(" + ")
}

fn format_cost_entry(cost: &UsageCostEntry) -> String {
    if !cost.total.is_finite() || cost.total < 0.0 {
        return "—".to_string();
    }
    let symbol = match cost.currency.as_str() {
        "USD" => "$".to_string(),
        "CNY" => "¥".to_string(),
        currency => format!("{currency} "),
    };
    let amount = if cost.total == 0.0 || cost.total >= 0.01 {
        format!("{:.2}", cost.total)
    } else {
        format!("{:.4}", cost.total)
    };
    let estimate = if cost.basis == UsageCostBasis::ApiEquivalent {
        "~"
    } else {
        ""
    };
    format!("{estimate}{symbol}{amount}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_context_placeholders() {
        assert_eq!(format_context(None, None), "—/—");
        assert_eq!(format_context(Some(12_200), None), "12.2k/—");
        assert_eq!(format_context(None, Some(200_000)), "—/200k");
        assert_eq!(format_context(Some(12_200), Some(200_000)), "12.2k/200k");
    }

    #[test]
    fn context_fill_sums_input_and_cache_read() {
        let usage = Usage {
            input: 10_000,
            output: 50,
            cache_read: 3_000,
            cache_write: 0,
            total_tokens: 13_050,
            units: Default::default(),
            cost: Default::default(),
        };
        assert_eq!(context_fill_from_usage(&usage), 13_000);
        assert!(usage_has_signal(&usage));
    }
}
