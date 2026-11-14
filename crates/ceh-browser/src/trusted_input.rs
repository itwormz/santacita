//! Internal typed-action primitive. Never accepts UI/page-provided selectors or scripts.
use crate::{
    BrowserError, DispatchGate, DispatchOutcome,
    bidi::{BidiConnection, ProtocolError},
    dom::{FrameSource, INSPECT_DOCUMENT, read_document},
    observer::{ObservedPage, PageObserver},
};
use ceh_core::{model::{ActionKind, DocumentGuard, PageKind}, recognition::PageSnapshot};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
const GUARD: &str = include_str!("mutation-guard.js");
/// Resolve an element in the guarded realm, reobserve, then publish one trusted
/// pointer sequence bound to that element reference. No coordinate or JS fallback.
/// Acknowledgement is not evidence of a page transition; the core observes next.
#[derive(Clone, Copy)]
enum TargetOrigin {
    Icp,
    Provider,
}
impl TargetOrigin {
    fn predicate(self) -> &'static str {
        match self {
            Self::Icp => "origin=>origin==='https://icp.administracionelectronica.gob.es'",
            Self::Provider => {
                "origin=>origin==='https://pasarela.clave.gob.es'||origin==='https://pasarela-ident.clave.gob.es'"
            }
        }
    }
}
pub(crate) struct Target<'a> {
    action: ActionKind,
    payload: Value,
    script: &'static str,
    origin: TargetOrigin,
    source: Option<FrameSource>,
    published: Option<&'a mut (dyn FnMut() + Send)>,
}
impl Target<'static> {
    pub fn recovery(payload: Value, source: FrameSource) -> Self {
        Self {
            action: ActionKind::Recovery,
            payload,
            script: include_str!("recovery-step.js"),
            origin: TargetOrigin::Icp,
            source: Some(source),
            published: None,
        }
    }
    pub fn icp(action: ActionKind, payload: Value, script: &'static str) -> Self {
        Self {
            action,
            payload,
            script,
            origin: TargetOrigin::Icp,
            source: None,
            published: None,
        }
    }
}
impl<'a> Target<'a> {
    pub fn provider(
        payload: Value,
        script: &'static str,
        published: Option<&'a mut (dyn FnMut() + Send)>,
    ) -> Self {
        Self {
            action: ActionKind::Auth,
            payload,
            script,
            origin: TargetOrigin::Provider,
            source: None,
            published,
        }
    }
}
pub(crate) fn click(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
) -> Result<DispatchOutcome, BrowserError> {
    activate(connection, observer, gate, guard, fresh, target, true)
}
pub(crate) fn focus(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
) -> Result<DispatchOutcome, BrowserError> {
    activate(connection, observer, gate, guard, fresh, target, false)
}
fn activate(
    connection: &mut BidiConnection,
    observer: &mut PageObserver,
    gate: &DispatchGate,
    guard: &DocumentGuard,
    fresh: ObservedPage,
    target: Target<'_>,
    submission: bool,
) -> Result<DispatchOutcome, BrowserError> {
    let Target {
        action,
        mut payload,
        script: target_script,
        origin,
        source,
        mut published,
    } = target;
    let mut permit = gate.begin_preparation(guard, &fresh.observation, action)?;
    let fill_mode = if payload["fillMode"] == "copy" {
        "copy"
    } else {
        "manual"
    };
    let scoped_document = if let Some(source) = &source {
        if !fresh
            .page
            .sources
            .iter()
            .any(|s| s.context == source.context && s.realm == source.realm)
        {
            return Err(stale("source_missing_from_observation"));
        }
        let document = read_document(connection, &source.context, fill_mode)
            .map_err(|_| unseen("source_read_failed"))?;
        if document.realm != source.realm {
            return Err(stale("source_realm_changed"));
        }
        Some(document)
    } else {
        None
    };
    let snapshot = scoped_document
        .as_ref()
        .map(|d| &d.snapshot)
        .unwrap_or(&fresh.page.snapshot);
    let frame = snapshot.frames.first().ok_or(BrowserError::NoObservation)?;
    let context = source
        .as_ref()
        .or_else(|| fresh.page.sources.first())
        .ok_or(BrowserError::NoObservation)?
        .context
        .clone();
    let realm = source
        .as_ref()
        .map(|s| &s.realm)
        .unwrap_or(&fresh.observation.document);
    let allowed = origin.predicate();
    let declaration = format!(
        "raw=>{{const inspect={INSPECT_DOCUMENT};const validate={GUARD};const operation={target_script};const allowed={allowed};const target=validate(raw,inspect,operation,allowed);return target;}}"
    );
    payload["expectedText"] = json!(frame.text);
    payload["expectedVisible"] = json!(snapshot.visible);
    payload["expectedEndpoint"] = json!(snapshot.endpoint);
    let mut ticket = None;
    let sent=permit.send(&fresh.observation,||{
  let Some(now)=SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|d|u64::try_from(d.as_millis()).ok()) else{return DispatchOutcome::NotDispatched;};
  payload["issuedAt"]=json!(now);
  let params=json!({"target":{"realm":realm.as_str()},"awaitPromise":false,"functionDeclaration":declaration,"arguments":[{"type":"string","value":payload.to_string()}]});
  send(connection,"script.callFunction",params,&mut ticket)
 })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(permit.finish());
    };
    let result = match connection.receive(ticket) {
        Ok(result) => result,
        Err(_) => {
            permit.complete_step(DispatchOutcome::Unknown)?;
            return Ok(permit.finish());
        }
    };
    if result["type"] != "success" {
        permit.complete_step(DispatchOutcome::Unknown)?;
        return Ok(permit.finish());
    }
    if result["result"]["type"] != "node" {
        permit.complete_step(DispatchOutcome::NotDispatched)?;
        return Ok(permit.finish());
    }
    // The node resolved; a person scrolls it into view with the wheel just before the click (below).
    permit.complete_step(DispatchOutcome::Dispatched)?;
    let Some(shared) = result["result"]["sharedId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
    else {
        return Ok(DispatchOutcome::NotDispatched);
    };
    let shared = shared.to_owned();
    permit.finish();
    let current = observer
        .observe(connection, fill_mode)
        .map_err(|_| unseen("second_observation_failed"))?;
    if current.page.snapshot.endpoint != fresh.page.snapshot.endpoint
        || current.page.snapshot.visible != fresh.page.snapshot.visible
        || current.page.snapshot.frames.first().map(|f| &f.text)
            != fresh.page.snapshot.frames.first().map(|f| &f.text)
    {
        return Err(stale(describe_change(current.observation.kind, &fresh.page.snapshot, &current.page.snapshot)));
    }
    if let Some(source) = &source {
        if current.observation.document != guard.document
            || current.observation.kind != guard.expected_page
            || !current
                .page
                .sources
                .iter()
                .any(|s| s.context == source.context && s.realm == source.realm)
        {
            return Err(stale("document_kind_or_source_changed"));
        }
        let document = read_document(connection, &source.context, fill_mode)
            .map_err(|_| unseen("source_reread_failed"))?;
        if document.realm != source.realm
            || document.snapshot.endpoint != snapshot.endpoint
            || document.snapshot.visible != snapshot.visible
            || document.snapshot.frames.first().map(|f| &f.text)
                != snapshot.frames.first().map(|f| &f.text)
        {
            return Err(stale("source_text_or_realm_changed"));
        }
    }
    let mut permit = if submission {
        gate.begin(guard, &current.observation, action)?
    } else {
        gate.begin_preparation(guard, &current.observation, action)?
    };
    let mut geometry = element_geometry(connection, realm, &shared);
    if scroll_with_wheel(connection, &context, realm, &shared, &geometry) {
        geometry = element_geometry(connection, realm, &shared);
    }
    // From where the cursor was left (viewport coordinates), expressed relative to this element's centre.
    let from = geometry.centre.zip(connection.cursor()).map(|((cx, cy), (x, y))| ((x - cx).round() as i32, (y - cy).round() as i32));
    let plan = ceh_core::input::click_plan(pointer_seed(), geometry.reach, geometry.size, from);
    if let Some((cx, cy)) = geometry.centre {
        connection.set_cursor(Some((cx + plan.end.0 as f64, cy + plan.end.1 as f64)));
    }
    let params = json!({"context":context,"actions":[{"type":"pointer","id":"ceh-pointer","parameters":{"pointerType":"mouse"},"actions":click_actions(&shared, &plan)}]});
    let mut ticket = None;
    let sent = permit.send(&current.observation, || {
        send(connection, "input.performActions", params, &mut ticket)
    })?;
    // Notify outside the authority lock and before waiting for a possible native
    // certificate dialog. Hook is trusted, bounded controller bookkeeping only.
    if sent == DispatchOutcome::Unknown
        && let Some(hook) = published.as_mut()
    {
        hook();
    }
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(permit.finish());
    };
    let outcome = match connection.receive(ticket) {
        Ok(_) => DispatchOutcome::Dispatched,
        Err(_) => DispatchOutcome::Unknown,
    };
    permit.complete_step(outcome)?;
    Ok(permit.finish())
}
thread_local! {
    static STALE_REASON: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
/// Why the last click was refused as stale (which check), for the diagnostics timeline; read once.
pub fn take_stale_reason() -> Option<String> {
    STALE_REASON.with(|r| r.borrow_mut().take())
}
fn stale(reason: impl Into<String>) -> BrowserError {
    STALE_REASON.with(|r| *r.borrow_mut() = Some(reason.into()));
    BrowserError::StaleDocument
}
fn unseen(reason: &'static str) -> BrowserError {
    STALE_REASON.with(|r| *r.borrow_mut() = Some(reason.to_owned()));
    BrowserError::NoObservation
}
/// Which part of the page read changed between two reads: lengths, where the text first differs and, for the
/// public error pages only (no personal data there), a short snippet of both sides.
fn describe_change(kind: PageKind, before: &PageSnapshot, after: &PageSnapshot) -> String {
    let (a, b) = (before.frames.first().map_or("", |f| f.text.as_str()), after.frames.first().map_or("", |f| f.text.as_str()));
    let at = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    let mut text = format!(
        "page changed: text_equal={} visible_equal={} endpoint_equal={} len={}->{} first_diff={at}",
        a == b, before.visible == after.visible, before.endpoint == after.endpoint, a.chars().count(), b.chars().count()
    );
    if matches!(kind, PageKind::Expired | PageKind::ServerError | PageKind::Blocked | PageKind::RateLimited) {
        let cut = |t: &str| t.chars().skip(at.saturating_sub(20)).take(60).collect::<String>();
        text.push_str(&format!(" before={:?} after={:?}", cut(a), cut(b)));
    }
    text
}
/// The cursor travels the planned path to a point inside the element, hovers, presses and releases there.
pub(crate) fn click_actions(shared: &str, plan: &ceh_core::input::ClickPlan) -> Vec<Value> {
    let origin = json!({"type":"element","element":{"sharedId":shared}});
    let mut actions: Vec<Value> = plan
        .moves
        .iter()
        .map(|step| json!({"type":"pointerMove","duration":step.ms,"x":step.x,"y":step.y,"origin":origin}))
        .collect();
    actions.push(json!({"type":"pause","duration":plan.hover_ms}));
    actions.push(json!({"type":"pointerDown","button":0}));
    actions.push(json!({"type":"pause","duration":plan.press_ms}));
    actions.push(json!({"type":"pointerUp","button":0}));
    actions
}
/// Where the element is and how much room the cursor has around it inside the viewport. `None` fields stay
/// unknown when the geometry cannot be read: the click then collapses onto the centre.
struct ElementGeometry {
    viewport_height: f64,
    reach: ceh_core::input::Reach,
    size: ceh_core::input::ElementBox,
    centre: Option<(f64, f64)>,
}
fn element_geometry(connection: &mut BidiConnection, realm: &ceh_core::model::DocumentId, shared: &str) -> ElementGeometry {
    let unknown = ElementGeometry {
        viewport_height: 0.0,
        reach: ceh_core::input::Reach { left: 0, right: 0, up: 0, down: 0 },
        size: ceh_core::input::ElementBox { width: 0.0, height: 0.0 },
        centre: None,
    };
    let params = json!({"target":{"realm":realm.as_str()},"awaitPromise":false,"arguments":[{"sharedId":shared}],
        "functionDeclaration":"el=>{const r=el.getBoundingClientRect();const w=document.documentElement.clientWidth,h=document.documentElement.clientHeight;const cx=r.left+r.width/2,cy=r.top+r.height/2;return JSON.stringify([cx,w-cx,cy,h-cy,r.width,r.height]);}"});
    let Ok(result) = connection.call("script.callFunction", params) else { return unknown };
    let Some(v) = result["result"]["value"].as_str().and_then(|t| serde_json::from_str::<Vec<f64>>(t).ok()).filter(|v| v.len() == 6) else { return unknown };
    let room = |v: f64| (v.floor() as i32 - 2).max(0);
    ElementGeometry {
        viewport_height: v[2] + v[3],
        reach: ceh_core::input::Reach { left: room(v[0]), right: room(v[1]), up: room(v[2]), down: room(v[3]) },
        size: ceh_core::input::ElementBox { width: v[4], height: v[5] },
        centre: Some((v[0], v[2])),
    }
}
thread_local! {
    static SCROLL_FALLBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Whether the last click had to fall back to `scrollIntoView` because the wheel did not bring the element
/// into view (diagnostics); read once.
pub fn take_scroll_fallback() -> bool {
    SCROLL_FALLBACK.with(|f| f.replace(false))
}
/// Scrolls the page with wheel notches until the element is comfortably in view. A script's `scrollIntoView`
/// leaves a scroll with no wheel behind it, which is not what a person produces. When the wheel did not get
/// there (a scroll container that ignores it) the instant scroll is the last resort and is recorded.
/// Returns whether the page was scrolled.
fn scroll_with_wheel(connection: &mut BidiConnection, context: &str, realm: &ceh_core::model::DocumentId, shared: &str, geometry: &ElementGeometry) -> bool {
    let Some((cx, cy)) = geometry.centre else { return false };
    let seed = pointer_seed() ^ 0x5352;
    let needed = ceh_core::input::scroll_needed(seed, cy, geometry.size.height, geometry.viewport_height);
    if needed == 0 {
        return false;
    }
    let (x, y) = connection.cursor().map_or((cx, geometry.viewport_height / 2.0), |p| p);
    let (x, y) = (x.round().max(1.0) as i64, y.round().clamp(1.0, (geometry.viewport_height - 1.0).max(1.0)) as i64);
    let mut actions = Vec::new();
    for notch in ceh_core::input::wheel_plan(seed, needed) {
        actions.push(json!({"type":"scroll","x":x,"y":y,"deltaX":0,"deltaY":notch.delta,"duration":0,"origin":"viewport"}));
        actions.push(json!({"type":"pause","duration":notch.pause_ms}));
    }
    let params = json!({"context":context,"actions":[{"type":"wheel","id":"ceh-wheel","actions":actions}]});
    let _ = connection.call("input.performActions", params);
    // Smooth scrolling finishes after the last notch.
    std::thread::sleep(std::time::Duration::from_millis(150));
    let after = element_geometry(connection, realm, shared);
    if let Some((_, cy)) = after.centre
        && ceh_core::input::scroll_needed(seed, cy, after.size.height, after.viewport_height) != 0
    {
        SCROLL_FALLBACK.with(|f| f.set(true));
        let params = json!({"target":{"realm":realm.as_str()},"awaitPromise":false,"arguments":[{"sharedId":shared}],
            "functionDeclaration":"el=>{el.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});}"});
        let _ = connection.call("script.callFunction", params);
    }
    true
}
fn pointer_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}
fn send(
    connection: &mut BidiConnection,
    method: &str,
    params: Value,
    ticket: &mut Option<crate::bidi::Ticket>,
) -> DispatchOutcome {
    match connection.send(method, params) {
        Ok(value) => {
            *ticket = Some(value);
            DispatchOutcome::Unknown
        }
        Err(ProtocolError::InvalidData | ProtocolError::PendingCommand) => {
            DispatchOutcome::NotDispatched
        }
        Err(_) => DispatchOutcome::Unknown,
    }
}

pub(crate) fn key(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
    value: char,
    hold_ms: u64,
) -> Result<DispatchOutcome, BrowserError> {
    perform_keys(connection, permit, fresh, key_actions(value, hold_ms))
}
/// A key press: down, held for `hold_ms` (a person's finger does not lift at once), then up.
pub(crate) fn key_actions(value: char, hold_ms: u64) -> Vec<Value> {
    let mut actions = vec![json!({"type":"keyDown","value":value.to_string()})];
    if hold_ms > 0 {
        actions.push(json!({"type":"pause","duration":hold_ms}));
    }
    actions.push(json!({"type":"keyUp","value":value.to_string()}));
    actions
}
/// Key actions that type `text` one character at a time (a select's type-ahead needs them in one chain).
const BACKSPACE: &str = "\u{E003}";
const META: &str = "\u{E03D}";
const CONTROL: &str = "\u{E009}";
/// Key actions that select the whole field and delete it: the platform modifier held around `a`, then Backspace.
pub(crate) fn clear_actions(mac: bool) -> Vec<Value> {
    let modifier = if mac { META } else { CONTROL };
    vec![
        json!({"type":"keyDown","value":modifier}),
        json!({"type":"keyDown","value":"a"}),
        json!({"type":"keyUp","value":"a"}),
        json!({"type":"keyUp","value":modifier}),
        json!({"type":"keyDown","value":BACKSPACE}),
        json!({"type":"keyUp","value":BACKSPACE}),
    ]
}
/// Select-all and delete in the focused field with the browser's own keyboard input.
pub(crate) fn clear_field(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
) -> Result<DispatchOutcome, BrowserError> {
    perform_keys(connection, permit, fresh, clear_actions(cfg!(target_os = "macos")))
}
fn perform_keys(
    connection: &mut BidiConnection,
    permit: &mut crate::ActionPermit,
    fresh: &ObservedPage,
    actions: Vec<Value>,
) -> Result<DispatchOutcome, BrowserError> {
    let context = &fresh
        .page
        .sources
        .first()
        .ok_or(BrowserError::NoObservation)?
        .context;
    let params = json!({"context":context,"actions":[{"type":"key","id":"ceh-keyboard","actions":actions}]});
    let mut ticket = None;
    let sent = permit.send(&fresh.observation, || {
        send(connection, "input.performActions", params, &mut ticket)
    })?;
    let Some(ticket) = ticket else {
        if sent == DispatchOutcome::Unknown {
            permit.complete_step(sent)?;
        }
        return Ok(sent);
    };
    let outcome = match connection.receive(ticket) {
        Ok(_) => DispatchOutcome::Dispatched,
        Err(_) => DispatchOutcome::Unknown,
    };
    permit.complete_step(outcome)?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_click_moves_along_a_curve_hovers_then_presses_inside_the_element() {
        let plan = ceh_core::input::click_plan(42, ceh_core::input::Reach { left: 500, right: 500, up: 500, down: 500 }, ceh_core::input::ElementBox { width: 120.0, height: 30.0 }, Some((200, 90)));
        let actions = super::click_actions("node-1", &plan);
        let moves: Vec<_> = actions.iter().filter(|a| a["type"] == "pointerMove").collect();
        assert!(moves.len() >= 8, "several moves, not one jump");
        assert!(moves.iter().all(|m| m["origin"]["element"]["sharedId"] == "node-1"));
        let last = moves.last().unwrap();
        assert_eq!((last["x"].as_i64(), last["y"].as_i64()), (Some(plan.end.0 as i64), Some(plan.end.1 as i64)));
        let tail: Vec<_> = actions.iter().rev().take(4).map(|a| (a["type"].as_str().unwrap(), a["duration"].as_u64())).collect();
        assert_eq!(tail, [("pointerUp", None), ("pause", Some(plan.press_ms)), ("pointerDown", None), ("pause", Some(plan.hover_ms))]);
    }

    use super::*;
    const TALL_PAGE: &str = "<!doctype html><title>t</title><body style='margin:0'><div style='height:3000px'>spacer</div><button id='b' style='height:40px;width:160px'>go</button><div style='height:800px'></div><script>const note=k=>document.documentElement.setAttribute('data-log',(document.documentElement.getAttribute('data-log')||'')+k+',');addEventListener('wheel',()=>note('wheel'),{passive:true});addEventListener('scroll',()=>note('scroll'),{passive:true});</script>";
    /// A button 3000 px down is reached by wheel notches: the page sees `wheel` before every `scroll`.
    fn wheel_scrolls_to_a_button_below_the_screen(mut browser: crate::firefox::OwnedFirefox) {
        let context = browser.context().to_owned();
        let url = format!("data:text/html;base64,{}", base64_encode(TALL_PAGE.as_bytes()));
        browser.connection().call("browsingContext.navigate", json!({"context":context,"url":url,"wait":"complete"})).unwrap();
        let node = browser.connection().call("script.evaluate", json!({"expression":"document.getElementById('b')","target":{"context":context},"awaitPromise":false,"resultOwnership":"root"})).unwrap();
        let shared = node["result"]["sharedId"].as_str().unwrap().to_owned();
        let realm = ceh_core::model::DocumentId::new(node["realm"].as_str().unwrap()).unwrap();
        let geometry = element_geometry(browser.connection(), &realm, &shared);
        assert!(geometry.centre.unwrap().1 > geometry.viewport_height, "below the fold to begin with");
        assert!(scroll_with_wheel(browser.connection(), &context, &realm, &shared, &geometry));
        let after = element_geometry(browser.connection(), &realm, &shared);
        let (_, y) = after.centre.unwrap();
        assert!(y > 0.0 && y < after.viewport_height, "in view now: {y} of {}", after.viewport_height);
        assert!(!take_scroll_fallback(), "the wheel was enough");
        let log = browser.connection().call("script.evaluate", json!({"expression":"document.documentElement.getAttribute('data-log')||''","target":{"context":context},"awaitPromise":false})).unwrap();
        let log: Vec<String> = log["result"]["value"].as_str().unwrap().split(',').filter(|e| !e.is_empty()).map(str::to_owned).collect();
        assert!(log.iter().filter(|e| *e == "wheel").count() >= 10, "{log:?}");
        assert_eq!(log.first().map(String::as_str), Some("wheel"), "the first scroll is caused by a wheel: {log:?}");
        let _ = browser.stop();
    }
    fn base64_encode(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..4 {
                if i <= chunk.len() { out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char) } else { out.push('=') }
            }
        }
        out
    }
    #[test]
    #[ignore = "launches the installed Firefox"]
    fn firefox_scrolls_with_the_wheel_before_a_click() {
        use crate::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
        let Ok(installation) = FirefoxInstallation::discover() else { return };
        let runtime = tempfile::tempdir().unwrap();
        let lease = ProfileLease::temporary(runtime.path()).unwrap();
        wheel_scrolls_to_a_button_below_the_screen(OwnedFirefox::start(&installation, lease, false, &std::sync::atomic::AtomicBool::new(false)).unwrap());
    }
    #[test]
    #[ignore = "launches the installed Chrome"]
    fn chrome_scrolls_with_the_wheel_before_a_click() {
        use crate::firefox::{OwnedFirefox, ProfileKind, ProfileLease};
        let Ok(installation) = crate::chromium::ChromiumInstallation::discover(ProfileKind::Chrome) else { return };
        let runtime = tempfile::tempdir().unwrap();
        let lease = ProfileLease::temporary_kind(runtime.path(), ProfileKind::Chrome).unwrap();
        wheel_scrolls_to_a_button_below_the_screen(OwnedFirefox::start_chromium(&installation, lease, &std::sync::atomic::AtomicBool::new(false)).unwrap());
    }
    #[test]
    fn a_key_is_held_between_down_and_up_only_when_a_hold_is_set() {
        let held: Vec<_> = key_actions('7', 64).iter().map(|a| (a["type"].as_str().unwrap().to_owned(), a["duration"].as_u64())).collect();
        assert_eq!(held, [("keyDown".to_owned(), None), ("pause".to_owned(), Some(64)), ("keyUp".to_owned(), None)]);
        assert_eq!(key_actions('7', 0).len(), 2);
    }
    #[test]
    fn clear_holds_the_platform_modifier_around_a_then_deletes() {
        for (mac, modifier) in [(true, META), (false, CONTROL)] {
            let actions = clear_actions(mac);
            let sequence: Vec<_> = actions.iter().map(|a| (a["type"].as_str().unwrap(), a["value"].as_str().unwrap())).collect();
            assert_eq!(sequence, vec![
                ("keyDown", modifier), ("keyDown", "a"), ("keyUp", "a"), ("keyUp", modifier),
                ("keyDown", BACKSPACE), ("keyUp", BACKSPACE),
            ]);
        }
    }

    /// Launches the installed Firefox (headless, temporary profile) on a 127.0.0.1 page: run with `-- --ignored`.
    #[test]
    #[ignore = "launches the installed Firefox"]
    fn real_firefox_reports_only_trusted_events_for_clear_text_and_select() {
        use crate::firefox::{FirefoxInstallation, OwnedFirefox, ProfileLease};
        use std::io::{Read, Write};
        let Ok(installation) = FirefoxInstallation::discover() else { return };
        let page = include_str!("../tests/fixtures/trusted-events.html");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            }
        });
        let runtime = tempfile::tempdir().unwrap();
        let lease = ProfileLease::temporary(runtime.path()).unwrap();
        let mut browser = OwnedFirefox::start(&installation, lease, true, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let context = browser.context().to_owned();
        let call = |browser: &mut OwnedFirefox, method: &str, params: Value| {
            let result = browser.connection().call(method, params).unwrap();
            let _ = browser.connection().take_events();
            result
        };
        call(&mut browser, "browsingContext.navigate", json!({"context":context,"url":url,"wait":"complete"}));
        let eval = |browser: &mut OwnedFirefox, expression: &str| -> Value {
            let result = call(browser, "script.evaluate", json!({"expression":format!("JSON.stringify({expression})"),"target":{"context":context},"awaitPromise":false}));
            serde_json::from_str(result["result"]["value"].as_str().unwrap()).unwrap()
        };
        let text_actions = |text: &str| -> Vec<Value> {
            text.chars().flat_map(|c| [json!({"type":"keyDown","value":c.to_string()}), json!({"type":"keyUp","value":c.to_string()})]).collect()
        };
        let keys = |browser: &mut OwnedFirefox, actions: Vec<Value>| {
            browser.connection().call("input.performActions", json!({"context":context,"actions":[{"type":"key","id":"k","actions":actions}]})).unwrap();
            let _ = browser.connection().take_events();
            std::thread::sleep(std::time::Duration::from_millis(150));
        };
        // Test-only focus; production focus is a trusted pointer press.
        let focus = |browser: &mut OwnedFirefox, id: &str| {
            let _ = browser.connection().call("script.evaluate", json!({"expression":format!("document.getElementById('{id}').focus()"),"target":{"context":context},"awaitPromise":false}));
        };
        focus(&mut browser, "txtIdCitado");
        keys(&mut browser, text_actions("Y1234567X"));
        assert_eq!(eval(&mut browser, "document.getElementById('txtIdCitado').value"), json!("Y1234567X"));
        keys(&mut browser, clear_actions(cfg!(target_os = "macos")));
        assert_eq!(eval(&mut browser, "document.getElementById('txtIdCitado').value"), json!(""));
        let events = eval(&mut browser, "window.__events.filter(e=>['input'].includes(e.type)).map(e=>e.trusted)");
        assert!(events.as_array().unwrap().len() >= 9 && events.as_array().unwrap().iter().all(|v| v == true), "{events}");
        assert_eq!(eval(&mut browser, "window.__events.filter(e=>!e.trusted).length"), json!(0));
        let _ = browser.stop();
    }
}
