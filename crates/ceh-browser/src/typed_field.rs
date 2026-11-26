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

thread_local! {
    static FAILED_AT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
/// Where the last field entry stopped (which check, which field operation), for the diagnostics timeline; read once.
/// Never holds a typed value: only fixed names and counts.
pub fn take_preparation_reason() -> Option<String> {
    FAILED_AT.with(|r| r.borrow_mut().take())
}
pub(crate) fn stopped(outcome: PreparationOutcome, reason: impl Into<String>) -> PreparationOutcome {
    FAILED_AT.with(|r| *r.borrow_mut() = Some(reason.into()));
    outcome
}
/// How often one field may read the page again because its text changed while the field was being entered
/// (the site shows and hides hints such as "the addresses do not match" while the second email is typed).
const PAGE_REREADS: usize = 12;
/// The page read a field entry works against, the preparation permit bound to it, and how often it was read again.
struct Entry {
    page: ObservedPage,
    permit: Option<crate::ActionPermit>,
    rereads: usize,
}
/// A field check that the page refused is asked once more against a fresh read of the same document: the guard
/// compares the whole page text, and a hint the site shows while typing changes it without touching the field.
/// The step script still checks the field itself (focus, value typed so far). Another document, address or page
/// kind ends the entry as before.
#[allow(clippy::too_many_arguments)]
fn check(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    field: &Field<'_>,
    fresh_page: FreshPage<'_>,
    entry: &mut Entry,
    operation: &str,
    prefix: &str,
) -> Result<Receipt, BrowserError> {
    let payload = (field.payload)(operation, prefix);
    let permit = entry.permit.as_mut().ok_or(BrowserError::InvalidControl)?;
    let receipt = mutation::perform(connection, permit, &entry.page, payload.clone(), field.script)?;
    if receipt != Receipt::Refused || entry.rereads >= PAGE_REREADS {
        return Ok(receipt);
    }
    let next = fresh_page(connection, observer)?;
    let (before, after) = (&entry.page.page.snapshot, &next.page.snapshot);
    let same_read = after.visible == before.visible && after.frames.first().map(|f| &f.text) == before.frames.first().map(|f| &f.text);
    if next.observation.document != guard.document || after.endpoint != before.endpoint || same_read {
        return Ok(receipt);
    }
    entry.rereads += 1;
    // The reservation of the old read is released before the new read takes it.
    entry.permit = None;
    entry.permit = Some(gate.begin_preparation(guard, &next.observation, field.action)?);
    entry.page = next;
    let permit = entry.permit.as_mut().ok_or(BrowserError::InvalidControl)?;
    mutation::perform(connection, permit, &entry.page, payload, field.script)
}
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
    FAILED_AT.with(|r| r.borrow_mut().take());
    let outcome = enter_field(connection, observer, gate, guard, field, fresh_page, typing_delay, deadline);
    // The field's fixed name (`email2`, `number`), never its value.
    let name = (field.payload)("verify", "")["field"].as_str().filter(|n| n.len() <= 16 && n.chars().all(|c| c.is_ascii_alphanumeric())).map(str::to_owned);
    if let Some(name) = name {
        FAILED_AT.with(|r| {
            if let Some(reason) = r.borrow_mut().as_mut() {
                *reason = format!("{name}: {reason}");
            }
        });
    }
    outcome
}
#[allow(clippy::too_many_arguments)]
fn enter_field(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    field: &Field<'_>,
    fresh_page: FreshPage<'_>,
    typing_delay: Duration,
    deadline: Instant,
) -> Result<PreparationOutcome, BrowserError> {
    use PreparationOutcome::{Failed, Unknown};
    if Instant::now() >= deadline {
        return Ok(stopped(Failed, "out_of_time_before_field"));
    }
    let fresh = fresh_page(connection, observer)?;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, field.action)?;
    let verified = mutation::perform(connection, &mut permit, &fresh, (field.payload)("verify", ""), field.script)?;
    if verified == Receipt::Unknown {
        return Ok(stopped(Unknown, "verify_unknown"));
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
        DispatchOutcome::Unknown => return Ok(stopped(Unknown, "focus_click_unknown")),
        DispatchOutcome::NotDispatched => return Ok(stopped(Failed, "focus_click_not_sent")),
        DispatchOutcome::Dispatched => {}
    }
    let fresh = fresh_page(connection, observer)?;
    let permit = gate.begin_preparation(guard, &fresh.observation, field.action)?;
    let mut state = Entry { page: fresh, permit: Some(permit), rereads: 0 };
    // Never send select-all + delete unless the pointer press really put the focus on this field.
    match check(connection, observer, gate, guard, field, fresh_page, &mut state, "focused", "")? {
        Receipt::Unchanged => {}
        Receipt::Unknown => return Ok(stopped(Unknown, "focus_check_unknown")),
        _ => return Ok(stopped(Failed, "field_not_focused")),
    }
    let cleared = {
        let Entry { page, permit, .. } = &mut state;
        trusted_input::clear_field(connection, permit.as_mut().ok_or(BrowserError::InvalidControl)?, page)?
    };
    match cleared {
        DispatchOutcome::Unknown => return Ok(stopped(Unknown, "clear_unknown")),
        DispatchOutcome::NotDispatched => return Ok(stopped(Failed, "clear_not_sent")),
        DispatchOutcome::Dispatched => {}
    }
    match check(connection, observer, gate, guard, field, fresh_page, &mut state, "empty", "")? {
        Receipt::Unchanged => {}
        Receipt::Unknown => return Ok(stopped(Unknown, "empty_check_unknown")),
        _ => return Ok(stopped(Failed, "field_not_empty")),
    }
    let mut prefix = String::new();
    let count = field.value.chars().count();
    let rhythm = ceh_core::input::typing_rhythm(rhythm_seed(), typing_delay.as_millis() as u64, count);
    // A person looks at the focused field before the first key.
    pause(Duration::from_millis(rhythm.first_ms), deadline);
    for (index, (character, stroke)) in field.value.chars().zip(&rhythm.keys).enumerate() {
        if Instant::now() >= deadline {
            return Ok(stopped(Failed, format!("out_of_time key={index}:{count}")));
        }
        match check(connection, observer, gate, guard, field, fresh_page, &mut state, "ready_key", &prefix)? {
            Receipt::Unchanged => {}
            Receipt::Unknown => return Ok(stopped(Unknown, format!("key_check_unknown key={index}:{count}"))),
            _ => return Ok(stopped(Failed, format!("field_changed_while_typing key={index}:{count} rereads={}", state.rereads))),
        }
        let pressed = {
            let Entry { page, permit, .. } = &mut state;
            trusted_input::key(connection, permit.as_mut().ok_or(BrowserError::InvalidControl)?, page, character, stroke.hold_ms)?
        };
        match pressed {
            DispatchOutcome::Unknown => return Ok(stopped(Unknown, format!("key_unknown key={index}:{count}"))),
            DispatchOutcome::NotDispatched => return Ok(stopped(Failed, format!("key_not_sent key={index}:{count}"))),
            DispatchOutcome::Dispatched => {}
        }
        prefix.push(character);
        pause(Duration::from_millis(stroke.pause_ms), deadline);
    }
    match check(connection, observer, gate, guard, field, fresh_page, &mut state, "verify", "")? {
        Receipt::Unchanged => Ok(PreparationOutcome::Completed),
        Receipt::Unknown => Ok(stopped(Unknown, "final_verify_unknown")),
        _ => Ok(stopped(Failed, format!("value_differs_after_typing rereads={}", state.rereads))),
    }
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
    /// The site's contact form as it behaves: a sticky header that changes its text once the page scrolls, the
    /// fields below the fold, and a hint under the repeated address while it does not match the first one yet.
    const CONTACT_PAGE: &str = "<!doctype html><title>t</title><body style='margin:0'><header id='h' style='position:sticky;top:0;background:#fff'>CITA PREVIA EXTRANJERIA</header><div style='height:1600px'>INFORMACION COMPLEMENTARIA</div><form>Telefono <input id='txtTelefonoCitado' type='text'><br>Correo <input id='emailUNO' type='text'><br>Repite <input id='emailDOS' type='text'><div id='hint'></div><input type='button' id='btnSiguiente' value='Siguiente'></form><div style='height:800px'></div><script>addEventListener('scroll',()=>{h.textContent=scrollY>40?'CITA PREVIA EXTRANJERIA Usuario CLAVE':'CITA PREVIA EXTRANJERIA';},{passive:true});emailDOS.addEventListener('input',()=>{hint.textContent=emailDOS.value===emailUNO.value?'':'Los correos no coinciden';});</script>";
    /// The applicant form: the same sticky header, the NIE type chosen, the number field below the fold.
    const APPLICANT_PAGE: &str = "<!doctype html><title>t</title><body style='margin:0'><header id='h' style='position:sticky;top:0;background:#fff'>CITA PREVIA EXTRANJERIA</header><div style='height:1600px'>Por favor, introduce los siguientes datos</div><form><input type='radio' name='rdbTipoDoc' id='rdbTipoDocNie' checked> N.I.E.<br>N.I.E. <input id='txtIdCitado' type='text' maxlength='9'><br>Nombre <input id='txtDesCitado' type='text'><input type='button' id='btnEnviar' value='Aceptar'></form><div style='height:800px'></div><script>addEventListener('scroll',()=>{h.textContent=scrollY>40?'CITA PREVIA EXTRANJERIA Usuario CLAVE':'CITA PREVIA EXTRANJERIA';},{passive:true});</script>";
    /// Serves `page` for every request on a local port; returns `host:port`.
    fn serve(page: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            }
        });
        host
    }
    fn open(browser: &mut crate::firefox::OwnedFirefox, page: &'static str, path: &str) -> (PageObserver, DispatchGate, DocumentGuard, ObservedPage) {
        let host = serve(page);
        crate::TEST_SITE_HOST.with(|h| *h.borrow_mut() = Some(host.clone()));
        let context = browser.context().to_owned();
        browser.connection().call("browsingContext.navigate", serde_json::json!({"context":context,"url":format!("http://{host}{path}"),"wait":"complete"})).unwrap();
        let mut observer = PageObserver::new(&context).unwrap();
        let fresh = observer.observe(browser.connection(), "manual").unwrap();
        let session = ceh_core::model::SessionId::new("typing-test").unwrap();
        let guard = DocumentGuard { session: session.clone(), epoch: 0, document: fresh.observation.document.clone(), expected_page: fresh.observation.kind };
        (observer, DispatchGate::new(session), guard, fresh)
    }
    fn value(browser: &mut crate::firefox::OwnedFirefox, id: &str) -> String {
        let context = browser.context().to_owned();
        let result = browser.connection().call("script.evaluate", serde_json::json!({"expression":format!("document.getElementById('{id}').value"),"target":{"context":context},"awaitPromise":false})).unwrap();
        result["result"]["value"].as_str().unwrap_or_default().to_owned()
    }
    /// The contact form is filled to the end although the page text changes under the repeated address while it
    /// is typed and the header changes when the page scrolls to the fields (the second address used to stop).
    fn the_contact_form_is_filled_while_the_page_shows_hints(mut browser: crate::firefox::OwnedFirefox) {
        let (mut observer, gate, guard, fresh) = open(&mut browser, CONTACT_PAGE, "/icpplustiem/acInfo");
        assert_eq!(fresh.observation.kind, ceh_core::model::PageKind::Contact);
        let cases: serde_json::Value = serde_json::from_str(include_str!("../../../contracts/fixtures/profile-validation.json")).unwrap();
        let mut profile: ceh_core::profiles::ProfileDraft = serde_json::from_value(cases["cases"][0]["expected"]["normalized"].clone()).unwrap();
        profile.phone = "600123456".into();
        profile.email = "alex.repeated@example.invalid".into();
        let input = crate::contact::ContactInput::from_profile(&profile).unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let outcome = crate::contact::prepare_contact(browser.connection(), &mut observer, &gate, &guard, &input, Duration::from_millis(20), deadline).unwrap();
        let stopped = take_preparation_reason();
        assert_eq!(outcome, PreparationOutcome::Completed, "entry stopped: {stopped:?}");
        assert_eq!(value(&mut browser, "txtTelefonoCitado"), "600123456");
        assert_eq!(value(&mut browser, "emailUNO"), "alex.repeated@example.invalid");
        assert_eq!(value(&mut browser, "emailDOS"), "alex.repeated@example.invalid");
        crate::TEST_SITE_HOST.with(|h| *h.borrow_mut() = None);
        let _ = browser.stop();
    }
    /// The NIE field below the fold is focused and typed although scrolling to it changes the page's header.
    fn the_nie_is_typed_after_the_scroll_changed_the_header(mut browser: crate::firefox::OwnedFirefox) {
        let (mut observer, gate, guard, fresh) = open(&mut browser, APPLICANT_PAGE, "/icpplustiem/acEntrada");
        assert_eq!(fresh.observation.kind, ceh_core::model::PageKind::Applicant);
        drop(fresh);
        let payload = |operation: &str, prefix: &str| serde_json::json!({"operation":operation,"field":"number","prefix":prefix,"fillMode":"manual","regreso":false,"copy":false,"documentType":"NIE","number":"Y1234567X","name":"ALEX","nationality":"X"});
        let field = Field { action: ActionKind::Applicant, script: include_str!("applicant-step.js"), value: "Y1234567X", payload: &payload };
        let page = |connection: &mut BidiConnection, observer: &mut PageObserver| {
            let fresh = observer.observe(connection, "manual").map_err(|_| BrowserError::NoObservation)?;
            if fresh.observation.kind != ceh_core::model::PageKind::Applicant { return Err(BrowserError::StaleDocument); }
            Ok(fresh)
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        let outcome = enter(browser.connection(), &mut observer, &gate, &guard, &field, &page, Duration::from_millis(20), deadline);
        let stopped = take_preparation_reason();
        assert!(matches!(outcome, Ok(PreparationOutcome::Completed)), "entry: {:?} stopped: {stopped:?} stale: {:?}", outcome.map_err(|e| e.code()), crate::trusted_input::take_stale_reason());
        assert_eq!(value(&mut browser, "txtIdCitado"), "Y1234567X");
        crate::TEST_SITE_HOST.with(|h| *h.borrow_mut() = None);
        let _ = browser.stop();
    }
    fn firefox() -> Option<crate::firefox::OwnedFirefox> {
        use crate::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
        let Ok(installation) = FirefoxInstallation::discover() else { crate::skip_missing_browser("Firefox"); return None };
        let runtime = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let lease = ProfileLease::temporary(runtime.path()).unwrap();
        Some(OwnedFirefox::start(&installation, lease, false, &std::sync::atomic::AtomicBool::new(false)).unwrap())
    }
    fn chrome() -> Option<crate::firefox::OwnedFirefox> {
        use crate::firefox::{OwnedFirefox, ProfileKind, ProfileLease};
        let Ok(installation) = crate::chromium::ChromiumInstallation::discover(ProfileKind::Chrome) else { crate::skip_missing_browser("Chrome"); return None };
        let runtime = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let lease = ProfileLease::temporary_kind(runtime.path(), ProfileKind::Chrome).unwrap();
        Some(OwnedFirefox::start_chromium(&installation, lease, &std::sync::atomic::AtomicBool::new(false)).unwrap())
    }
    #[test]
    #[ignore = "launches the installed Firefox"]
    fn firefox_fills_the_contact_form_while_the_page_shows_hints() {
        if let Some(browser) = firefox() { the_contact_form_is_filled_while_the_page_shows_hints(browser) }
    }
    #[test]
    #[ignore = "launches the installed Chrome"]
    fn chrome_fills_the_contact_form_while_the_page_shows_hints() {
        if let Some(browser) = chrome() { the_contact_form_is_filled_while_the_page_shows_hints(browser) }
    }
    #[test]
    #[ignore = "launches the installed Firefox"]
    fn firefox_types_the_nie_after_the_scroll_changed_the_header() {
        if let Some(browser) = firefox() { the_nie_is_typed_after_the_scroll_changed_the_header(browser) }
    }
    #[test]
    #[ignore = "launches the installed Chrome"]
    fn chrome_types_the_nie_after_the_scroll_changed_the_header() {
        if let Some(browser) = chrome() { the_nie_is_typed_after_the_scroll_changed_the_header(browser) }
    }

}
