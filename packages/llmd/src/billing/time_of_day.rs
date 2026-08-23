use std::str::FromStr;

use chrono::{Datelike, FixedOffset, NaiveTime};
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
    /// ISO-8601 weekday numbers the window applies to (1 = Monday .. 7 = Sunday).
    /// An empty list means the window applies every day of the week.
    #[serde(default)]
    pub days: Vec<u8>,
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
            validate_days(&window.days, index)?;
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
        let local = context.occurred_at.with_timezone(&offset);
        let local_time = local.time();
        let local_weekday = local.weekday().number_from_monday() as u8;
        let mut selected = &schedule.default;
        for window in &schedule.windows {
            if window_contains(window, local_weekday, local_time)? {
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

fn validate_days(days: &[u8], index: usize) -> Result<(), String> {
    let mut seen = [false; 8];
    for &day in days {
        if !(1..=7).contains(&day) {
            return Err(format!(
                "Time-of-day window {index} has weekday {day}, outside ISO 1..=7"
            ));
        }
        let slot = &mut seen[day as usize];
        if *slot {
            return Err(format!(
                "Time-of-day window {index} lists weekday {day} more than once"
            ));
        }
        *slot = true;
    }
    Ok(())
}

fn day_allowed(window: &TimeWindowPricing, weekday: u8) -> bool {
    window.days.is_empty() || window.days.contains(&weekday)
}

fn previous_iso_weekday(weekday: u8) -> u8 {
    if weekday == 1 { 7 } else { weekday - 1 }
}

fn window_contains(
    window: &TimeWindowPricing,
    weekday: u8,
    time: NaiveTime,
) -> Result<bool, String> {
    let (start, end) = parse_window(window)?;
    Ok(if start < end {
        day_allowed(window, weekday) && time >= start && time < end
    } else {
        (day_allowed(window, weekday) && time >= start)
            || (day_allowed(window, previous_iso_weekday(weekday)) && time < end)
    })
}

fn windows_overlap(left: &TimeWindowPricing, right: &TimeWindowPricing) -> Result<bool, String> {
    for weekday in 1..=7 {
        let left_intervals = window_intervals(left, weekday)?;
        let right_intervals = window_intervals(right, weekday)?;
        for &(left_start, left_end) in &left_intervals {
            for &(right_start, right_end) in &right_intervals {
                if interval_overlaps(left_start, left_end, right_start, right_end) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

// A window's coverage on a given weekday as half-open intervals. An `None` end
// means the interval reaches the end of the day (`[start, 24:00)`).
fn window_intervals(
    window: &TimeWindowPricing,
    weekday: u8,
) -> Result<Vec<(NaiveTime, Option<NaiveTime>)>, String> {
    let (start, end) = parse_window(window)?;
    let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let mut intervals = Vec::new();
    if start < end {
        if day_allowed(window, weekday) {
            intervals.push((start, Some(end)));
        }
    } else {
        if day_allowed(window, weekday) {
            intervals.push((start, None));
        }
        if day_allowed(window, previous_iso_weekday(weekday)) {
            intervals.push((midnight, Some(end)));
        }
    }
    Ok(intervals)
}

fn interval_overlaps(
    left_start: NaiveTime,
    left_end: Option<NaiveTime>,
    right_start: NaiveTime,
    right_end: Option<NaiveTime>,
) -> bool {
    let left_starts_before_right_end = right_end.is_none_or(|end| left_start < end);
    let right_starts_before_left_end = left_end.is_none_or(|end| right_start < end);
    left_starts_before_right_end && right_starts_before_left_end
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
        windows: Vec<([&str; 2], [f64; 3], Vec<u8>)>,
    ) -> BillingPlan {
        let schedule = TimeOfDayPricing {
            utc_offset: utc_offset.into(),
            default: tests_support::schedule(default),
            windows: windows
                .into_iter()
                .map(|([start, end], rates, days)| TimeWindowPricing {
                    start: start.into(),
                    end: end.into(),
                    rates: tests_support::schedule(rates),
                    days,
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
                (["09:00", "12:00"], [3.0, 0.10, 9.0], vec![]),
                (["14:00", "18:00"], [3.0, 0.10, 9.0], vec![]),
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
            vec![(["09:00", "12:00"], [3.0, 0.10, 9.0], vec![])],
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
            vec![(["22:00", "06:00"], [3.0, 0.10, 9.0], vec![])],
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
            vec![(["09:00", "09:00"], [2.0, 0.2, 4.0], vec![])],
        );
        assert!(validate(&same_bounds).is_err());
        let overlapping = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![
                (["09:00", "12:00"], [2.0, 0.2, 4.0], vec![]),
                (["11:00", "14:00"], [3.0, 0.3, 6.0], vec![]),
            ],
        );
        assert!(validate(&overlapping).is_err());
        // Abutting windows are not overlapping.
        let abutting = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![
                (["09:00", "12:00"], [2.0, 0.2, 4.0], vec![]),
                (["12:00", "14:00"], [3.0, 0.3, 6.0], vec![]),
            ],
        );
        assert!(validate(&abutting).is_ok());
    }

    #[test]
    fn peak_window_rates_only_apply_on_weekdays() {
        let plan = plan(
            "+08:00",
            [1.5, 0.05, 4.5],
            vec![
                (["09:00", "12:00"], [3.0, 0.10, 9.0], vec![1, 2, 3, 4, 5]),
                (["14:00", "18:00"], [3.0, 0.10, 9.0], vec![1, 2, 3, 4, 5]),
            ],
        );
        // Thu 2026-08-20: Beijing 10:30 and 15:00 are weekdays -> peak.
        assert_eq!(estimate_at(&plan, "2026-08-20T02:30:00"), 12.0);
        assert_eq!(estimate_at(&plan, "2026-08-20T07:00:00"), 12.0);
        // Fri 2026-08-21: Beijing 10:30 is a weekday -> peak.
        assert_eq!(estimate_at(&plan, "2026-08-21T02:30:00"), 12.0);
        // Sat 2026-08-22: Beijing 10:30 and 15:00 fall on the weekend -> off-peak.
        assert_eq!(estimate_at(&plan, "2026-08-22T02:30:00"), 6.0);
        assert_eq!(estimate_at(&plan, "2026-08-22T07:00:00"), 6.0);
        // Sun 2026-08-23: Beijing 10:30 falls on the weekend -> off-peak.
        assert_eq!(estimate_at(&plan, "2026-08-23T02:30:00"), 6.0);
    }

    #[test]
    fn weekday_windows_on_disjoint_days_do_not_overlap() {
        let plan = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![
                (["09:00", "12:00"], [2.0, 0.2, 4.0], vec![1, 2, 3, 4, 5]),
                (["09:00", "12:00"], [3.0, 0.3, 6.0], vec![6, 7]),
            ],
        );
        assert!(validate(&plan).is_ok());
    }

    #[test]
    fn validation_rejects_invalid_weekday_days() {
        let out_of_range = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![(["09:00", "12:00"], [2.0, 0.2, 4.0], vec![0])],
        );
        assert!(validate(&out_of_range).is_err());
        let duplicate = plan(
            "+08:00",
            [1.0, 0.1, 2.0],
            vec![(["09:00", "12:00"], [2.0, 0.2, 4.0], vec![1, 1])],
        );
        assert!(validate(&duplicate).is_err());
    }
}
