use ceh_core::{flow::*, model::*, recovery::RecoveryAction};
fn observation(kind: PageKind) -> Observation {
    Observation { document: DocumentId::new("doc").unwrap(), kind, method: RequestMethod::Get, operation_code: false, variant: String::new() }
}
fn engine() -> FlowEngine {
    FlowEngine::new(SessionId::new("session").unwrap(), SearchOptions::default())
}
/// One block on a fresh engine: returns the wait the core schedules, then runs the cooldown out.
fn blocked_wait(engine: &mut FlowEngine) -> u64 {
    let FlowEffect::WaitUntil(deadline) = engine.observe(observation(PageKind::Blocked), 0).unwrap()[0].clone() else { panic!("no wait") };
    let effects = engine.observe(observation(PageKind::Blocked), deadline).unwrap();
    assert!(matches!(effects[0], FlowEffect::Recover { action: RecoveryAction::Restart { clear_profile: true, reopen: false }, .. }));
    deadline
}

#[test]
fn the_first_block_starts_a_new_session_at_once_and_every_block_that_comes_back_waits_610_seconds() {
    let mut carry = RecoveryCarry::default();
    let mut waits = Vec::new();
    for _ in 0..4 {
        let mut next = engine();
        next.apply_carry(carry.clone());
        waits.push(blocked_wait(&mut next));
        carry = next.recovery_carry();
    }
    assert_eq!(waits, vec![0, 610_000, 610_000, 610_000]);
}

#[test]
fn without_the_carry_every_new_session_would_start_at_once() {
    let waits: Vec<u64> = (0..2).map(|_| blocked_wait(&mut engine())).collect();
    assert_eq!(waits, vec![0, 0]);
}

#[test]
fn a_progress_page_resets_the_blocked_streak_and_the_429_penalty_is_carried() {
    let mut first = engine();
    blocked_wait(&mut first);
    assert_eq!(first.recovery_carry().blocked_streak, 1);
    first.observe(observation(PageKind::Applicant), 31_000).unwrap();
    assert_eq!(first.recovery_carry().blocked_streak, 0);
    let carry = RecoveryCarry { blocked_streak: 2, applicant_reload_penalty: 60.0 };
    let mut second = engine();
    second.apply_carry(carry.clone());
    assert_eq!(second.recovery_carry(), carry);
    assert_eq!(second.applicant_reload_penalty(), 60.0);
}
