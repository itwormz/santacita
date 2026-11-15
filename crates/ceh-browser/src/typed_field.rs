//! One text field entered the way a person does: focus by pointer, select-all and delete by keyboard,
//! then one real key press per character. Page scripts only read: they verify the field and resolve it.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::BidiConnection,
    mutation::{self, Receipt},
    observer::{ObservedPage, PageObserver},
    trusted_input::{self, Target},
};
use ceh_core::{
    flow::PreparationOutcome,
    model::{ActionKind, DocumentGuard},
};
use serde_json::Value;
use std::time::{Duration, Instant};

/// The step script understands the read-only operations `verify`, `target`, `focused`, `empty` and `ready_key`.
pub(crate) struct Field<'a> {
    pub action: ActionKind,
    pub script: &'static str,
    pub value: &'a str,
    /// `(operation, prefix typed so far)` -> script payload.
    pub payload: &'a dyn Fn(&str, &str) -> Value,
}
pub(crate) type FreshPage<'a> =
    &'a dyn Fn(&mut BidiConnection, &mut PageObserver) -> Result<ObservedPage, BrowserError>;

// The step is one typed operation; its inputs are all distinct and a struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enter(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    field: &Field<'_>,
    fresh_page: FreshPage<'_>,
    typing_delay: Duration,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    if Instant::now() >= deadline {
        return Ok(PreparationOutcome::Failed);
    }
    let mut fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, field.action)?;
    let verified = mutation::perform(connection, &mut permit, &fresh, (field.payload)("verify", ""), field.script)?;
    if verified == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if verified == Receipt::Unchanged {
        return Ok(PreparationOutcome::Completed);
    }
    drop(permit);
    let focused = trusted_input::focus(
        connection,
        observer,
        gate,
        guard,
        fresh,
        Target::icp(field.action, (field.payload)("target", ""), field.script),
    )?;
    match focused {
        DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
        DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
        DispatchOutcome::Dispatched => {}
    }
    fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, field.action)?;
    // Never send select-all + delete unless the pointer press really put the focus on this field.
    let focused = mutation::perform(connection, &mut permit, &fresh, (field.payload)("focused", ""), field.script)?;
    if focused == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if focused != Receipt::Unchanged {
        return Ok(PreparationOutcome::Failed);
    }
    match trusted_input::clear_field(connection, &mut permit, &fresh)? {
        DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
        DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
        DispatchOutcome::Dispatched => {}
    }
    let emptied = mutation::perform(connection, &mut permit, &fresh, (field.payload)("empty", ""), field.script)?;
    if emptied == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if emptied != Receipt::Unchanged {
        return Ok(PreparationOutcome::Failed);
    }
    let mut prefix = String::new();
    let rhythm = ceh_core::input::typing_rhythm(rhythm_seed(), typing_delay.as_millis() as u64, field.value.chars().count());
    // A person looks at the focused field before the first key.
    pause(Duration::from_millis(rhythm.first_ms), deadline);
    for (character, stroke) in field.value.chars().zip(&rhythm.keys) {
        if Instant::now() >= deadline {
            return Ok(PreparationOutcome::Failed);
        }
        let ready = mutation::perform(connection, &mut permit, &fresh, (field.payload)("ready_key", &prefix), field.script)?;
        if ready == Receipt::Unknown {
            return Ok(PreparationOutcome::Unknown);
        }
        if ready != Receipt::Unchanged {
            return Ok(PreparationOutcome::Failed);
        }
        match trusted_input::key(connection, &mut permit, &fresh, character, stroke.hold_ms)? {
            DispatchOutcome::Unknown => return Ok(PreparationOutcome::Unknown),
            DispatchOutcome::NotDispatched => return Ok(PreparationOutcome::Failed),
            DispatchOutcome::Dispatched => {}
        }
        prefix.push(character);
        pause(Duration::from_millis(stroke.pause_ms), deadline);
    }
    let verified = mutation::perform(connection, &mut permit, &fresh, (field.payload)("verify", ""), field.script)?;
    if verified == Receipt::Unknown {
        return Ok(PreparationOutcome::Unknown);
    }
    if verified != Receipt::Unchanged {
        return Ok(PreparationOutcome::Failed);
    }
    Ok(PreparationOutcome::Completed)
}

fn pause(wait: Duration, deadline: Instant) {
    if !wait.is_zero() {
        std::thread::sleep(wait.min(deadline.saturating_duration_since(Instant::now())));
    }
}
fn rhythm_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}
/// Time a step may spend typing on top of its navigation budget: the rhythm of a person (a hold and a pause per
/// key, a look at the field before the first one) takes 0.6 s per character and 0.8 s per field with room to
/// spare, so a long value does not run out of time halfway.
pub(crate) fn typing_budget(characters: usize, fields: usize) -> Duration {
    Duration::from_millis(characters as u64 * 600 + fields as u64 * 800)
}
/// Longest pause between two typed characters: a person never types slower than the typing-ability spec
/// allows, but a long configured pause must not abort the step (the spec accepts up to 1000 ms).
pub(crate) const MAX_TYPING_DELAY: Duration = Duration::from_millis(250);
pub(crate) fn typing_delay(configured: Duration) -> Duration {
    configured.min(MAX_TYPING_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_typing_budget_covers_the_generated_rhythm_of_every_field_of_a_long_contact_step() {
        // A phone and a 120-character email typed twice, at the slowest configurable typing pace.
        let characters = 9 + 2 * 120;
        let budget = typing_budget(characters, 3);
        for seed in 0..200u64 {
            let mut total = 0u64;
            for count in [9usize, 120, 120] {
                let rhythm = ceh_core::input::typing_rhythm(seed + count as u64, MAX_TYPING_DELAY.as_millis() as u64, count);
                total += rhythm.first_ms + rhythm.keys.iter().map(|k| k.hold_ms + k.pause_ms).sum::<u64>();
            }
            assert!(Duration::from_millis(total) < budget, "seed {seed}: {total} ms against a budget of {budget:?}");
        }
        assert_eq!(typing_budget(0, 0), Duration::ZERO);
    }
    #[test]
    fn a_long_configured_typing_delay_is_clamped_not_rejected() {
        assert_eq!(typing_delay(Duration::from_millis(70)), Duration::from_millis(70));
        assert_eq!(typing_delay(Duration::from_millis(300)), MAX_TYPING_DELAY);
        assert_eq!(typing_delay(Duration::from_millis(1000)), MAX_TYPING_DELAY);
        assert_eq!(typing_delay(Duration::ZERO), Duration::ZERO);
    }
}
