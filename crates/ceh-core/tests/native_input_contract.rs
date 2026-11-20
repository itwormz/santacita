use ceh_core::input::{selection_keys,SelectionKey,POINTER_BOUNDS};
#[test]
fn selection_requires_backend_verified_commit_without_form_submission(){
 assert_eq!(selection_keys(0,true).unwrap(),vec![SelectionKey::Home,SelectionKey::Commit]);
 assert_eq!(selection_keys(2,true).unwrap(),vec![SelectionKey::Home,SelectionKey::Down,SelectionKey::Down,SelectionKey::Commit]);
 assert!(selection_keys(257,true).is_err());assert!(selection_keys(2,false).is_err());
 const { assert!(POINTER_BOUNDS.hover_ms.0 > 0 && POINTER_BOUNDS.press_ms.0 > 0 && POINTER_BOUNDS.hover_ms.1 + POINTER_BOUNDS.press_ms.1 < 1000) };
}

use ceh_core::input::{click_plan, ClickPlan, ElementBox, PointerStep, Reach};
const OPEN: Reach = Reach { left: 2000, right: 2000, up: 2000, down: 2000 };
const BUTTON: ElementBox = ElementBox { width: 200.0, height: 40.0 };
fn dist(a: (i32, i32), b: (i32, i32)) -> f64 {
    (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
}
fn total(plan: &ClickPlan) -> u64 {
    plan.moves.iter().map(|s| s.ms).sum()
}

#[test]
fn the_click_lands_inside_the_inner_part_of_the_element_and_not_always_on_the_centre() {
    let mut xs = Vec::new();
    let mut distinct = std::collections::HashSet::new();
    for seed in 0..1000u64 {
        let plan = click_plan(seed, OPEN, BUTTON, None);
        assert!(plan.end.0.abs() <= 70 && plan.end.1.abs() <= 14, "seed {seed}: {:?} outside the inner 70 %", plan.end);
        xs.push(plan.end.0 as f64);
        distinct.insert(plan.end);
    }
    assert!(distinct.len() > 50, "the offsets vary, got {} different", distinct.len());
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    assert!(mean.abs() < 5.0, "centred on the middle, mean {mean}");
    // The last move arrives at the chosen point.
    let plan = click_plan(9, OPEN, BUTTON, None);
    let last = plan.moves.last().unwrap();
    assert_eq!((last.x, last.y), plan.end);
}

#[test]
fn a_small_element_is_hit_near_its_centre() {
    for seed in 0..200u64 {
        let plan = click_plan(seed, OPEN, ElementBox { width: 10.0, height: 10.0 }, None);
        assert!(plan.end.0.abs() <= 2 && plan.end.1.abs() <= 2, "{:?}", plan.end);
    }
}

#[test]
fn the_path_starts_where_the_cursor_was_and_not_at_a_random_point() {
    let from = (300, -80);
    for seed in 0..100u64 {
        let plan = click_plan(seed, OPEN, BUTTON, Some(from));
        let first = plan.moves.first().unwrap();
        let reach_of_trip = dist(from, plan.end);
        assert!(dist((first.x, first.y), from) < reach_of_trip * 0.3, "seed {seed}: first move {first:?} jumps away from the previous position");
    }
    // The very first move of a session has no previous position and starts away from the target.
    for seed in 0..50u64 {
        let plan = click_plan(seed, OPEN, BUTTON, None);
        let start = plan.moves.first().unwrap();
        assert!(dist((start.x, start.y), plan.end) >= 60.0, "seed {seed}");
    }
}

#[test]
fn the_path_is_deterministic_per_seed_and_bends() {
    assert_eq!(click_plan(5, OPEN, BUTTON, None), click_plan(5, OPEN, BUTTON, None));
    assert_ne!(click_plan(1, OPEN, BUTTON, None), click_plan(2, OPEN, BUTTON, None));
    let plan = click_plan(3, OPEN, BUTTON, Some((400, 200)));
    let first = plan.moves.first().unwrap();
    let off_chord = plan.moves.iter().any(|s: &PointerStep| {
        let cross = ((first.x - plan.end.0) as i64 * (s.y - plan.end.1) as i64 - (first.y - plan.end.1) as i64 * (s.x - plan.end.0) as i64).abs();
        cross > 300
    });
    assert!(off_chord, "the path bends");
}

#[test]
fn a_longer_trip_takes_longer_by_fitts_law_and_steps_are_unequal() {
    let median = |from: (i32, i32)| {
        let mut all: Vec<u64> = (0..101u64).map(|seed| total(&click_plan(seed, OPEN, BUTTON, Some(from)))).collect();
        all.sort_unstable();
        all[50]
    };
    let (near, far) = (median((120, 0)), median((900, 0)));
    assert!(far > near + 80, "near {near} ms, far {far} ms");
    assert!((250..=1400).contains(&near) && (250..=1800).contains(&far));
    let plan = click_plan(4, OPEN, BUTTON, Some((600, 100)));
    assert!(plan.moves.len() >= 12, "steps are about 16 ms apart like a 60 Hz mouse");
    let distinct: std::collections::HashSet<u64> = plan.moves.iter().map(|s| s.ms).collect();
    assert!(distinct.len() >= 3, "the step times differ: {distinct:?}");
    assert!(plan.moves.iter().all(|s| s.ms >= 1));
}

#[test]
fn hover_and_press_times_are_random_inside_their_bounds() {
    let mut hovers = std::collections::HashSet::new();
    let mut presses = std::collections::HashSet::new();
    for seed in 0..300u64 {
        let plan = click_plan(seed, OPEN, BUTTON, None);
        assert!((15..=90).contains(&plan.hover_ms), "{}", plan.hover_ms);
        assert!((50..=140).contains(&plan.press_ms), "{}", plan.press_ms);
        hovers.insert(plan.hover_ms);
        presses.insert(plan.press_ms);
    }
    assert!(hovers.len() > 30 && presses.len() > 30, "not constants");
}

#[test]
fn some_clicks_overshoot_by_a_few_pixels_and_come_back() {
    let mut overshoots = 0;
    for seed in 0..1000u64 {
        let plan = click_plan(seed, OPEN, BUTTON, Some((500, 150)));
        let n = plan.moves.len();
        let before_last = &plan.moves[n - 2];
        let gap = dist((before_last.x, before_last.y), plan.end);
        // Beyond the target along the direction of travel (an ordinary approach arrives from this side).
        let beyond = (before_last.x - plan.end.0) as f64 * (plan.end.0 - 500) as f64 + (before_last.y - plan.end.1) as f64 * (plan.end.1 - 150) as f64 > 0.0;
        if gap >= 2.0 && beyond {
            assert!((2.0..=9.5).contains(&gap), "seed {seed}: gap {gap}");
            overshoots += 1;
        }
    }
    assert!((60..=260).contains(&overshoots), "about 15 % of the clicks, got {overshoots}/1000");
}

#[test]
fn the_cursor_never_leaves_the_room_the_element_has_in_the_viewport() {
    // An element near the left edge, 200 px from it, 40 px from the top.
    let reach = Reach { left: 198, right: 900, up: 38, down: 600 };
    for seed in 0..300u64 {
        for from in [None, Some((-700, -500)), Some((1500, 900))] {
            let plan = click_plan(seed, reach, BUTTON, from);
            for step in &plan.moves {
                assert!(step.x >= -198 && step.x <= 900 && step.y >= -38 && step.y <= 600, "seed {seed}: {step:?}");
            }
            let end = plan.end;
            assert!(end.0 >= -198 && end.0 <= 900 && end.1 >= -38 && end.1 <= 600);
        }
    }
    // No room at all (unknown geometry): everything collapses onto the centre instead of leaving the page.
    let none = Reach { left: 0, right: 0, up: 0, down: 0 };
    let plan = click_plan(3, none, BUTTON, None);
    assert!(plan.moves.iter().all(|s| (s.x, s.y) == (0, 0)) && plan.end == (0, 0));
}

use ceh_core::input::typing_rhythm;

fn field_ms(r: &ceh_core::input::TypingRhythm) -> u64 {
    r.first_ms + r.keys.iter().map(|k| k.hold_ms + k.pause_ms).sum::<u64>()
}

#[test]
fn typing_has_its_own_hold_and_pause_for_every_key() {
    let rhythm = typing_rhythm(7, 90, 20);
    assert_eq!(rhythm.keys.len(), 20);
    assert!((100..=320).contains(&rhythm.first_ms), "{}", rhythm.first_ms);
    let pauses: std::collections::HashSet<u64> = rhythm.keys.iter().map(|k| k.pause_ms).collect();
    let holds: std::collections::HashSet<u64> = rhythm.keys.iter().map(|k| k.hold_ms).collect();
    assert!(pauses.len() >= 8, "intervals are not all equal: {pauses:?}");
    assert!(holds.len() >= 8, "holds are not all equal: {holds:?}");
    assert!(rhythm.keys.iter().all(|k| (12..=130).contains(&k.hold_ms) && k.pause_ms >= 6), "{:?}", rhythm.keys);
    assert_eq!(typing_rhythm(7, 90, 20), typing_rhythm(7, 90, 20), "a seed gives the same rhythm");
}

#[test]
fn a_field_never_takes_more_than_seventy_percent_longer_than_the_fixed_delay_it_replaced() {
    // The profile's typing delay used to be the pause between two keys: a field took count x delay.
    for median in [35u64, 52, 70, 105, 140] {
        for count in [4usize, 9, 20, 22] {
            let old = (count as u64 * median) as f64;
            let mut sum = 0.0;
            for seed in 0..300u64 {
                let total = field_ms(&typing_rhythm(seed, median, count)) as f64;
                assert!(total <= old * 1.7, "delay {median}, {count} keys, seed {seed}: {total} ms against {old} ms");
                sum += total;
            }
            let mean = sum / 300.0 / old;
            assert!((1.25..=1.6).contains(&mean), "delay {median}, {count} keys: on average x{mean:.2} of the old time");
        }
    }
}

#[test]
fn the_intervals_vary_and_now_and_then_a_key_waits_noticeably_longer() {
    let mut intervals: Vec<f64> = (0..200u64).flat_map(|seed| typing_rhythm(seed, 90, 20).keys.into_iter().map(|k| (k.hold_ms + k.pause_ms) as f64)).collect();
    let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
    let spread = (intervals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / intervals.len() as f64).sqrt() / mean;
    assert!(spread > 0.25, "a person's keys are not evenly spaced: spread {spread:.2}");
    intervals.sort_by(f64::total_cmp);
    let long = intervals.iter().filter(|v| **v > mean * 2.0).count() as f64 / intervals.len() as f64;
    assert!((0.005..0.08).contains(&long), "rare hesitations, {long:.3}");
}

#[test]
fn no_configured_typing_delay_means_no_added_time() {
    let rhythm = typing_rhythm(3, 0, 10);
    assert_eq!(rhythm.first_ms, 0);
    assert!(rhythm.keys.iter().all(|k| k.pause_ms == 0 && k.hold_ms == 0));
}

#[test]
fn typing_nothing_waits_for_nothing() {
    let rhythm = typing_rhythm(3, 90, 0);
    assert_eq!((rhythm.first_ms, rhythm.keys.len()), (0, 0));
}

use ceh_core::input::reading_pause_ms;

#[test]
fn a_page_is_read_before_the_first_action_longer_when_it_has_more_text() {
    for seed in 0..200u64 {
        let empty = reading_pause_ms(seed, 0);
        assert!((400..=1000).contains(&empty), "{empty}");
        let form = reading_pause_ms(seed, 100);
        assert!((1200..=1800).contains(&form), "100 words add about 0.8 s, got {form}");
        let wall_of_text = reading_pause_ms(seed, 5000);
        assert!(wall_of_text <= 1000 + 1500, "the words add at most 1.5 s, got {wall_of_text}");
    }
    assert_eq!(reading_pause_ms(4, 40), reading_pause_ms(4, 40));
    assert_ne!(reading_pause_ms(4, 40), reading_pause_ms(5, 40));
}

use ceh_core::input::{scroll_needed, wheel_plan};

#[test]
fn an_element_that_is_fully_in_view_is_not_scrolled_to() {
    // Viewport 800 px high, element 40 px high with its centre at 400: nothing to do.
    assert_eq!(scroll_needed(1, 400.0, 40.0, 800.0), 0);
    // Near the bottom edge (cut off) or the top edge: scroll.
    assert!(scroll_needed(1, 790.0, 40.0, 800.0) > 0);
    assert!(scroll_needed(1, 10.0, 40.0, 800.0) < 0);
    // Far below: bring it to the middle part of the viewport, not to the very edge.
    let needed = scroll_needed(1, 2000.0, 40.0, 800.0);
    assert!((2000 - 560..=2000 - 280).contains(&needed), "{needed}");
}

#[test]
fn the_wheel_scrolls_in_notches_of_about_a_hundred_pixels_with_pauses() {
    for seed in 0..100u64 {
        let plan = wheel_plan(seed, 1234);
        assert_eq!(plan.iter().map(|n| n.delta).sum::<i32>(), 1234, "exactly the needed distance");
        assert!(plan.len() >= 10);
        assert!(plan.iter().all(|n| (1..=120).contains(&n.delta)));
        assert!(plan[..plan.len() - 1].iter().all(|n| (80..=120).contains(&n.delta)), "{plan:?}");
        assert!(plan.iter().all(|n| (40..=120).contains(&n.pause_ms)));
    }
    let up = wheel_plan(2, -500);
    assert_eq!(up.iter().map(|n| n.delta).sum::<i32>(), -500);
    assert!(wheel_plan(2, 0).is_empty());
    assert_ne!(wheel_plan(1, 900), wheel_plan(2, 900));
}

use ceh_core::input::{idle_motion, idle_motion_allowed};
use ceh_core::control::ControlMode;

#[test]
fn a_waiting_page_gets_a_few_small_cursor_movements_every_few_seconds() {
    for seed in 0..200u64 {
        let motion = idle_motion(seed);
        assert!((2000..=8000).contains(&motion.next_in_ms), "{}", motion.next_in_ms);
        assert!((1..=3).contains(&motion.moves.len()));
        for step in &motion.moves {
            assert!(step.x.abs() <= 14 && step.y.abs() <= 14 && (step.x, step.y) != (0, 0), "{step:?}");
            assert!((80..=250).contains(&step.ms));
        }
    }
    assert_ne!(idle_motion(1), idle_motion(2));
}

#[test]
fn the_cursor_is_never_moved_while_a_person_has_the_page_or_the_search_is_paused() {
    assert!(idle_motion_allowed(ControlMode::Automatic, true));
    assert!(!idle_motion_allowed(ControlMode::Automatic, false), "nothing is known about the pointer yet");
    assert!(!idle_motion_allowed(ControlMode::Manual, true));
    assert!(!idle_motion_allowed(ControlMode::Paused, true));
}

use ceh_core::input::clamp_idle_moves;

#[test]
fn idle_moves_never_leave_the_viewport_and_report_where_the_cursor_ends() {
    let steps = |v: &[(i32, i32)]| v.iter().map(|(x, y)| PointerStep { x: *x, y: *y, ms: 100 }).collect::<Vec<_>>();
    // Near the right and bottom edge: the offsets that would leave the page are cut short.
    let (moves, end) = clamp_idle_moves((795.0, 595.0), &steps(&[(10, 10), (10, 10), (-6, -6)]), (800.0, 600.0));
    assert_eq!(moves.iter().map(|m| (m.x, m.y)).collect::<Vec<_>>(), vec![(3, 3), (-6, -6)], "cut at the edge, and the move that changes nothing is dropped");
    assert_eq!(end, (792.0, 592.0), "ends where the sent moves end");
    let mut at = (795.0, 595.0);
    for m in &moves {
        at = (at.0 + m.x as f64, at.1 + m.y as f64);
        assert!(at.0 >= 2.0 && at.0 <= 798.0 && at.1 >= 2.0 && at.1 <= 598.0, "{at:?}");
    }
    assert_eq!(at, end);
    // A move that would not change anything is not sent.
    let (moves, end) = clamp_idle_moves((798.0, 598.0), &steps(&[(5, 5)]), (800.0, 600.0));
    assert!(moves.is_empty());
    assert_eq!(end, (798.0, 598.0));
    // Inside the page nothing changes.
    let (moves, end) = clamp_idle_moves((300.0, 300.0), &steps(&[(7, -4), (-3, 9)]), (800.0, 600.0));
    assert_eq!(moves.iter().map(|m| (m.x, m.y)).collect::<Vec<_>>(), vec![(7, -4), (-3, 9)]);
    assert_eq!(end, (304.0, 305.0));
}

#[test]
fn a_very_small_typing_delay_is_not_stretched_by_a_minimum_interval_per_key() {
    // 120 keys at 1 ms used to take at least 120 x 8 ms; the limit of 70 % longer holds for every delay.
    for (median, count) in [(1u64, 120usize), (1, 40), (2, 120), (3, 60), (5, 200)] {
        let old = (count as u64 * median) as f64;
        for seed in 0..200u64 {
            let rhythm = typing_rhythm(seed, median, count);
            let total = field_ms(&rhythm) as f64;
            assert!(total <= old * 1.7 + 1.0, "delay {median}, {count} keys, seed {seed}: {total} ms against {old} ms");
            assert!(rhythm.keys.iter().all(|k| k.hold_ms <= k.hold_ms + k.pause_ms), "a key is never held longer than its own time");
        }
    }
}

#[test]
fn the_field_stays_inside_its_budget_for_every_delay_including_the_ones_near_a_key_minimum() {
    // Around the point where a key's share is near the shortest time a key can take, and for few keys.
    for median in 1u64..=40 {
        for count in [1usize, 2, 3, 5, 9, 20, 60] {
            let old = (count as u64 * median) as f64;
            for seed in 0..150u64 {
                let rhythm = typing_rhythm(seed, median, count);
                let total = field_ms(&rhythm) as f64;
                assert!(total <= old * 1.7, "delay {median}, {count} keys, seed {seed}: {total} ms against {old} ms");
                assert!(rhythm.keys.iter().all(|k| k.hold_ms <= k.hold_ms + k.pause_ms), "{:?}", rhythm.keys);
            }
        }
    }
}
