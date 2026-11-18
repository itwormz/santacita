//! A stop that waits for a person always says where in the flow it happened (a short fixed code, never page data).
use ceh_core::{flow::*, model::*};

fn observation(kind: PageKind) -> Observation {
    Observation {
        document: DocumentId::new("doc").unwrap(),
        kind,
        method: RequestMethod::Get,
        operation_code: false,
        variant: String::new(),
    }
}
fn engine() -> FlowEngine {
    FlowEngine::new(SessionId::new("session").unwrap(), SearchOptions::default())
}

#[test]
fn a_new_engine_has_no_hold_reason() {
    let e = engine();
    let state = e.diagnostics();
    assert_eq!(state.hold_reason, None);
    assert_eq!(state.phase, "none");
    assert!(!state.manual);
}

#[test]
fn an_action_followed_by_the_same_page_is_held_with_its_reason() {
    let mut e = engine();
    let o = observation(PageKind::Applicant);
    let effects = e.observe(o.clone(), 0).unwrap();
    let FlowEffect::Prepare { guard, action } = &effects[0] else {
        panic!("missing preparation")
    };
    e.complete_preparation(guard, *action, PreparationOutcome::Completed).unwrap();
    assert!(matches!(&e.observe(o.clone(), 1).unwrap()[0], FlowEffect::Activate { .. }));
    assert_eq!(e.diagnostics().phase, "dispatched");
    assert_eq!(e.observe(o, 2).unwrap(), vec![FlowEffect::Hold]);
    let state = e.diagnostics();
    assert_eq!(state.hold_reason, Some("dispatched_page_unchanged"));
    assert_eq!(state.phase, "held");
}

#[test]
fn an_office_page_without_offices_is_held_with_its_reason() {
    let mut e = engine();
    let effects = e
        .observe_with_facts(observation(PageKind::Office), PageFacts { office_count: 0, ..Default::default() }, 0)
        .unwrap();
    assert_eq!(effects, vec![FlowEffect::Hold]);
    assert_eq!(e.diagnostics().hold_reason, Some("office_count_zero"));
}

#[test]
fn leaving_a_stop_clears_its_reason() {
    let mut e = engine();
    let held = e
        .observe_with_facts(observation(PageKind::Office), PageFacts { office_count: 0, ..Default::default() }, 0)
        .unwrap();
    assert_eq!(held, vec![FlowEffect::Hold]);
    assert_eq!(e.diagnostics().hold_reason, Some("office_count_zero"));
    // The page moves on: the old reason must not be reported for the new situation.
    let next = Observation { document: DocumentId::new("doc-2").unwrap(), ..observation(PageKind::Procedure) };
    assert!(matches!(e.observe(next, 1).unwrap()[0], FlowEffect::Prepare { .. }));
    assert_eq!(e.diagnostics().hold_reason, None);
}

#[test]
fn a_planned_wait_does_not_record_the_hold_it_may_end_in() {
    let mut e = engine();
    let o = observation(PageKind::RequestNoSlotsRestart);
    let effects = e.observe(o.clone(), 0).unwrap();
    assert!(matches!(effects[0], FlowEffect::WaitUntil(_)), "{effects:?}");
    assert_eq!(e.diagnostics().hold_reason, None, "still only waiting");
    // The restart control is still missing when the wait is over: now it is a stop.
    assert_eq!(e.observe(o, 10_000_000).unwrap(), vec![FlowEffect::Hold]);
    assert_eq!(e.diagnostics().hold_reason, Some("no_slots_restart_without_control"));
}

#[test]
fn a_page_with_no_handler_is_held_with_its_reason() {
    let mut e = engine();
    let effects = e.observe(observation(PageKind::Procedure), 0).unwrap();
    // Procedure is a prepared action, not a hold: the reason stays empty.
    assert!(matches!(effects[0], FlowEffect::Prepare { .. }));
    assert_eq!(e.diagnostics().hold_reason, None);
}
