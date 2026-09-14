//! Data-proportional lane strips (timelines).
//!
//! A lane strip is a stack of horizontal lanes sharing one main (x) axis.
//! Blocks inside each lane are placed by time proportion when every segment
//! carries a timed extent; otherwise the whole strip degrades to equal
//! sequence placement. Placement uses one span shared across all lanes so a
//! position in one lane maps to the same x in every lane. The solver is pure
//! geometry: it knows nothing about what a segment means.

use ratatui::layout::Rect;

/// Timed extent of one segment in caller-defined units (e.g. milliseconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneExtent {
    pub start: u64,
    pub length: u64,
}

/// One segment request. `extent` absent means timing is unavailable for this
/// segment; the whole strip then degrades to sequence placement.
#[derive(Clone, Copy, Debug)]
pub struct LaneSegment {
    pub extent: Option<LaneExtent>,
    /// Smallest painted width in cells (clamped, never past the area edge).
    pub min_width: u16,
}

/// One lane (row) of segments.
#[derive(Clone, Debug, Default)]
pub struct Lane {
    pub segments: Vec<LaneSegment>,
}

/// Solved geometry for one lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanePlan {
    /// One rect per segment of the lane, same order. Degenerate areas produce
    /// zero-width rects.
    pub rects: Vec<Rect>,
}

/// Solved lane strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneStripPlan {
    /// One plan per input lane, same order, vertically stacked inside `area`.
    pub lanes: Vec<LanePlan>,
    /// False when any segment lacked an extent: every lane is in sequence
    /// mode and the caller must say so instead of implying proportional time.
    pub timed: bool,
}

/// Solve a stacked lane strip within `area`.
///
/// The area is divided evenly between lanes (each lane is one row). With all
/// extents present, blocks are proportional to their extents over one span
/// shared by every lane; otherwise each lane distributes equal shares with
/// 1-cell gaps. Zero-width or zero-height areas yield zero-width rects.
pub fn solve_lane_strip(area: Rect, lanes: &[Lane]) -> LaneStripPlan {
    let segments: Vec<LaneSegment> = lanes
        .iter()
        .flat_map(|lane| lane.segments.iter().copied())
        .collect();
    let timed = !segments.is_empty() && segments.iter().all(|s| s.extent.is_some());
    let empty_plan = |count: usize| LanePlan {
        rects: vec![Rect::ZERO; count],
    };
    if area.width == 0 || area.height == 0 || segments.is_empty() {
        return LaneStripPlan {
            lanes: lanes
                .iter()
                .map(|lane| empty_plan(lane.segments.len()))
                .collect(),
            timed,
        };
    }
    let lane_height = area.height / lanes.len() as u16;
    let lane_rows = lanes
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let y = area.y.saturating_add(index as u16 * lane_height);
            Rect::new(area.x, y, area.width, lane_height)
        })
        .collect::<Vec<_>>();
    if timed {
        let extents = segments
            .iter()
            .map(|s| s.extent.expect("checked above"))
            .collect::<Vec<_>>();
        let span_start = extents.iter().map(|e| e.start).min().expect("non-empty");
        let span_end = extents
            .iter()
            .map(|e| e.start.saturating_add(e.length))
            .max()
            .expect("non-empty");
        let span = span_end.saturating_sub(span_start).max(1);
        let plans = lanes
            .iter()
            .zip(lane_rows)
            .map(|(lane, row)| {
                let rects = timed_rects(row, &lane.segments, span_start, span);
                LanePlan { rects }
            })
            .collect();
        LaneStripPlan {
            lanes: plans,
            timed: true,
        }
    } else {
        let plans = lanes
            .iter()
            .zip(lane_rows)
            .map(|(lane, row)| LanePlan {
                rects: sequence_rects(row, &lane.segments),
            })
            .collect();
        LaneStripPlan {
            lanes: plans,
            timed: false,
        }
    }
}

fn timed_rects(area: Rect, segments: &[LaneSegment], span_start: u64, span: u64) -> Vec<Rect> {
    let scale = u64::from(area.width);
    segments
        .iter()
        .map(|segment| {
            let Some(extent) = segment.extent else {
                return Rect::ZERO;
            };
            let offset = (extent.start.saturating_sub(span_start) * scale) / span;
            let width = (extent.length.max(1) * scale / span).max(u64::from(segment.min_width));
            let x = area.x.saturating_add((offset.min(scale)) as u16);
            let width = width.min(u64::from(area.right().saturating_sub(x)));
            Rect::new(x, area.y, width as u16, area.height)
        })
        .collect()
}

fn sequence_rects(area: Rect, segments: &[LaneSegment]) -> Vec<Rect> {
    let count = segments.len() as u16;
    let share = area.width / count;
    if share == 0 {
        // More segments than cells: one cell each from the left, rest zero-width.
        return segments
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if index as u32 >= u32::from(area.width) {
                    Rect::ZERO
                } else {
                    Rect::new(area.x + index as u16, area.y, 1, area.height)
                }
            })
            .collect();
    }
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let x = area.x.saturating_add(index as u16 * share);
            let width = share
                .saturating_sub(1)
                .max(segment.min_width.min(share))
                .min(area.right().saturating_sub(x));
            Rect::new(x, area.y, width, area.height)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timed(start: u64, length: u64) -> LaneSegment {
        LaneSegment {
            extent: Some(LaneExtent { start, length }),
            min_width: 1,
        }
    }

    fn untimed() -> LaneSegment {
        LaneSegment {
            extent: None,
            min_width: 1,
        }
    }

    #[test]
    fn proportional_widths_follow_extent() {
        // Observed span 10..=80 over 100 cells: 10..=30 -> x=0 w=28;
        // 60..=80 -> offset 50/70 of 100 -> x=71 w=28.
        let strip = solve_lane_strip(
            Rect::new(0, 0, 100, 1),
            &[Lane {
                segments: vec![timed(10, 20), timed(60, 20)],
            }],
        );
        assert!(strip.timed);
        assert_eq!(strip.lanes[0].rects[0], Rect::new(0, 0, 28, 1));
        assert_eq!(strip.lanes[0].rects[1], Rect::new(71, 0, 28, 1));
    }

    #[test]
    fn span_is_observed_not_zero_based() {
        // 50..=60 and 60..=70: span = 20 over 100 cells, first block starts
        // at the left edge with half the width.
        let strip = solve_lane_strip(
            Rect::new(0, 0, 100, 1),
            &[Lane {
                segments: vec![timed(50, 10), timed(60, 10)],
            }],
        );
        assert_eq!(strip.lanes[0].rects[0], Rect::new(0, 0, 50, 1));
        assert_eq!(strip.lanes[0].rects[1], Rect::new(50, 0, 50, 1));
    }

    #[test]
    fn lanes_stack_vertically_and_share_one_time_axis() {
        // Lane A: step 10..=30. Lane B: tool 20..=40. Shared span 10..=40.
        let strip = solve_lane_strip(
            Rect::new(0, 0, 30, 2),
            &[
                Lane {
                    segments: vec![timed(10, 20)],
                },
                Lane {
                    segments: vec![timed(20, 20)],
                },
            ],
        );
        assert!(strip.timed);
        assert_eq!(strip.lanes[0].rects[0], Rect::new(0, 0, 20, 1));
        assert_eq!(strip.lanes[1].rects[0], Rect::new(10, 1, 20, 1));
    }

    #[test]
    fn min_width_clamps_and_stays_in_area() {
        let strip = solve_lane_strip(
            Rect::new(0, 0, 10, 1),
            &[Lane {
                segments: vec![
                    LaneSegment {
                        extent: Some(LaneExtent {
                            start: 0,
                            length: 1,
                        }),
                        min_width: 4,
                    },
                    timed(9, 1),
                ],
            }],
        );
        assert_eq!(strip.lanes[0].rects[0].width, 4);
        assert!(strip.lanes[0].rects[1].right() <= 10);
    }

    #[test]
    fn any_absent_extent_degrades_every_lane_to_sequence_mode() {
        let strip = solve_lane_strip(
            Rect::new(0, 0, 9, 2),
            &[
                Lane {
                    segments: vec![timed(0, 5)],
                },
                Lane {
                    segments: vec![untimed()],
                },
            ],
        );
        assert!(!strip.timed);
        assert_eq!(strip.lanes[0].rects[0], Rect::new(0, 0, 8, 1));
        assert_eq!(strip.lanes[1].rects[0], Rect::new(0, 1, 8, 1));
    }

    #[test]
    fn sequence_mode_drops_overflow_segments() {
        let strip = solve_lane_strip(
            Rect::new(0, 0, 3, 1),
            &[Lane {
                segments: vec![untimed(), untimed(), untimed(), untimed()],
            }],
        );
        assert!(!strip.timed);
        assert_eq!(strip.lanes[0].rects[3], Rect::ZERO);
        assert_eq!(strip.lanes[0].rects[2], Rect::new(2, 0, 1, 1));
    }

    #[test]
    fn degenerate_areas_are_zero_width() {
        let strip = solve_lane_strip(
            Rect::new(0, 0, 0, 1),
            &[Lane {
                segments: vec![timed(0, 1)],
            }],
        );
        assert_eq!(strip.lanes[0].rects[0], Rect::ZERO);
        let strip = solve_lane_strip(
            Rect::new(0, 0, 4, 0),
            &[Lane {
                segments: vec![timed(0, 1)],
            }],
        );
        assert_eq!(strip.lanes[0].rects[0], Rect::ZERO);
    }
}
