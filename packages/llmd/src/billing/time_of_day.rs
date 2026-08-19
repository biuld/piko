use std::str::FromStr;

use chrono::{FixedOffset, NaiveTime};
use serde::{Deserialize, Serialize};

use super::standard::{estimate_standard, validate_schedule};
use super::{BillableUsage, BillingContext, PricingPolicy};
use crate::billing::standard::StandardTokenPricing;
use crate::modeling::BillingPlan;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeOfDayPricing {
    pub utc_offset: String,
    pub default: StandardTokenPricing,
    #[serde(default)]
    pub windows: Vec<TimeWindowPricing>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeWindowPricing {
    pub start: String,
    pub end: String,
    pub rates: StandardTokenPricing,
}

pub(super) struct TimeOfDayPolicy;

impl PricingPolicy for TimeOfDayPolicy {
    fn id(&self) -> &'static str {
        "time_of_day"
    }

    fn validate(&self, plan: &BillingPlan) -> Result<(), String> {
        let schedule = schedule(plan)?;
        parse_offset(&schedule.utc_offset)?;
        validate_schedule(&schedule.default)?;
        for (index, window) in schedule.windows.iter().enumerate() {
            let start = parse_time(&window.start, &format!("window {index} start"))?;
            let end = parse_time(&window.end, &format!("window {index} end"))?;
            if start == end {
                return Err(format!(
                    "Time-of-day window {index} must span at least one minute"
                ));
            }
            validate_schedule(&window.rates)?;
        }
        for left in 0..schedule.windows.len() {
            for right in left + 1..schedule.windows.len() {
                if windows_overlap(&schedule.windows[left], &schedule.windows[right])? {
                    return Err(format!("Time-of-day windows {left} and {right} overlap"));
                }
            }
        }
        Ok(())
    }

    fn estimate(
        &self,
        context: &BillingContext<'_>,
        plan: &BillingPlan,
        usage: &BillableUsage,
    ) -> Result<piko_protocol::messages::UsageCost, String> {
        let schedule = schedule(plan)?;
        let offset = parse_offset(&schedule.utc_offset)?;
        let local_time = context.occurred_at.with_timezone(&offset).time();
        let mut selected = &schedule.default;
        for window in &schedule.windows {
            if window_contains(window, local_time)? {
                selected = &window.rates;
                break;
            }
        }
        estimate_standard(&plan.currency, plan.basis, selected, usage)
    }
}

fn schedule(plan: &BillingPlan) -> Result<TimeOfDayPricing, String> {
    serde_json::from_value(plan.configuration.clone())
        .map_err(|error| format!("Invalid time-of-day pricing configuration: {error}"))
}

fn parse_offset(raw: &str) -> Result<FixedOffset, String> {
    FixedOffset::from_str(raw).map_err(|error| format!("Invalid UTC offset {raw:?}: {error}"))
}

fn parse_time(raw: &str, label: &str) -> Result<NaiveTime, String> {
    NaiveTime::from_str(raw).map_err(|error| format!("Invalid {label} {raw:?}: {error}"))
}

fn parse_window(window: &TimeWindowPricing) -> Result<(NaiveTime, NaiveTime), String> {
    Ok((
        parse_time(&window.start, "window start")?,
        parse_time(&window.end, "window end")?,
    ))
}

fn window_contains(window: &TimeWindowPricing, time: NaiveTime) -> Result<bool, String> {
    let (start, end) = parse_window(window)?;
    Ok(if start < end {
        time >= start && time < end
    } else {
        time >= start || time < end
    })
}

fn windows_overlap(left: &TimeWindowPricing, right: &TimeWindowPricing) -> Result<bool, String> {
    let (left_start, left_end) = parse_window(left)?;
    let (right_start, right_end) = parse_window(right)?;
    let contains = |time: NaiveTime, start: NaiveTime, end: NaiveTime| {
        if start < end {
            time >= start && time < end
        } else {
            time >= start || time < end
        }
    };
    Ok(contains(right_start, left_start, left_end) || contains(left_start, right_start, right_end))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::billing::tests_support;
    use crate::billing::{BillingContext, BillingRegistry};
    use crate::modeling::BillingPlan;

    use super::{TimeOfDayPricing, TimeWindowPricing};

    fn plan(
        utc_offset: &str,
        default: [f64; 3],
        windows: Vec<([&str; 2], [f64; 3])>,
    ) -> BillingPlan {
        let schedule = TimeOfDayPricing {
            utc_offset: utc_offset.into(),
            default: tests_support::schedule(default),
            windows: windows
                .into_iter()
                .map(|([start, end], rates)| TimeWindowPricing {
                    start: start.into(),
                    end: end.into(),
                    rates: tests_support::schedule(rates),
                })
                .collect(),
        };
        BillingPlan {
            usage_adapter: "semantic_tokens".into(),
            pricing_policy: "time_of_day".into(),
            currency: "CNY".into(),
            basis: piko_protocol::messages::UsageCostBasis::ListPrice,
            configuration: serde_json::to_value(schedule).unwrap(),
        }
    }

    fn estimate_at(plan: &BillingPlan, utc: &str) -> f64 {
        let registry = BillingRegistry::standard();
        let context = BillingContext {
            provider: "deepseek",
            model: "deepseek-v4-flash",
            api_surface: "platform",
            occurred_at: chrono::Utc.from_utc_datetime(
                &chrono::NaiveDateTime::parse_from_str(utc, "%Y-%m-%dT%H:%M:%S").unwrap(),
            ),
        };
        let cost = registry
            .estimate(
                &context,
                plan,
                &tests_support::usage(1_000_000, 1_000_000, 0, 0),
            )
            .unwrap();
        cost.entries[0].total
    }

    fn validate(plan: &BillingPlan) -> Result<(), String> {
        BillingRegistry::standard().validate(plan)
    }

    #[test]
    fn peak_window_rates_apply_during_beijing_peak_hours() {
        let plan = plan(
            "+08:00",
            [1.5, 0.05, 4.5],
            vec![
                (["09:00", "12:00"], [3.0, 0.10, 9.0]),
                (["14:00", "18:00"], [3.0, 0.10, 9.0]),
            ],
        );
        // Beijing 10:30 and 15:00 are peak.
        assert_eq!(estimate_at(&plan, "2026-08-20T02:30:00"), 12.0);
        assert_eq!(estimate_at(&plan, "2026-08-20T07:00:00"), 12.0);
        // Beijing 13:00 is off-peak.
        assert_eq!(estimate_at(&plan, "2026-08-20T05:00:00"), 6.0);
    }

    #[test]
    fn window_boundaries_are_half_open() {
        let plan = plan(
            "+08:00",
            [1.5, 0.05, 4.5],
            vec![(["09:00", "12:00"], [3.0, 0.10, 9.0])],
        );
        // Beijing 09:00 enters the peak window; 12:00 has exited it.
        assert_eq!(estimate_at(&plan, "2026-08-20T01:00:00"), 12.0);
        assert_eq!(estimate_at(&plan, "2026-08-20T04:00:00"), 6.0);
    }

    #[test]
    fn midnight_crossing_window_matches_both_sides() {
        let plan = plan(
            "+08:00",
            [1.5, 0.05, 4.5],
            vec![(["22:00", "06:00"], [3.0, 0.10, 9.0])],
        );
        assert_eq!(estimate_at(&plan, "2026-08-20T15:00:00"), 12.0); // Beijing 23:00
        assert_eq!(estimate_at(&plan, "2026-08-20T18:00:00"), 12.0); // Beijing 02:00
        assert_eq!(estimate_at(&plan, "2026-08-20T10:00:00"), 6.0); // Beijing 18:00
    }

    #[test]
    fn validation_rejects_invalid_and_overlapping_windows() {
        assert!(validate(&plan("not-an-offset", [1.0, 0.1, 2.0], vec![])).is_err());
        let same_bounds = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![(["09:00", "09:00"], [2.0, 0.2, 4.0])],
        );
        assert!(validate(&same_bounds).is_err());
        let overlapping = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![
                (["09:00", "12:00"], [2.0, 0.2, 4.0]),
                (["11:00", "14:00"], [3.0, 0.3, 6.0]),
            ],
        );
        assert!(validate(&overlapping).is_err());
        // Abutting windows are not overlapping.
        let abutting = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![
                (["09:00", "12:00"], [2.0, 0.2, 4.0]),
                (["12:00", "14:00"], [3.0, 0.3, 6.0]),
            ],
        );
        assert!(validate(&abutting).is_ok());
    }
}
