//! What a search remembers about itself, always and in memory only, so that a stop that waits for a person
//! can leave a report to copy (`startup_report`): where the flow stopped, the page it stopped on, the last
//! events and the settings that steer the behaviour. Names, flags, counts, durations and short hashes only:
//! no document number, name, phone, e-mail, office, cookie, page text or id (see the contract tests).
use ceh_core::{flow::FlowEffect, model::Observation, profiles::ProfileDraft};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque};

/// Events kept; older ones fall out.
const MAX_ROWS: usize = 40;
/// Only the newest page rows keep their full detail (selectors, facts): the report stays small.
const DETAILED_ROWS: usize = 3;
/// The store refuses reports above 128 KiB.
const MAX_BYTES: usize = 100 * 1024;

/// The profile settings that steer the flow. The province and the service are included on purpose: a change on
/// the site that the app cannot handle shows up there first. Identity and contact fields never are.
/// A stale reason as a plain code for the report. The browser adapter may append slices of the page text to it on error
/// pages (`before=...`, `after=...`): everything from there on is cut, and only plain code characters stay.
pub fn stale_code(reason: &str) -> String {
    let structured = reason.split(" before=").next().unwrap_or_default();
    structured
        .chars()
        .map(|c| if c == ' ' { ';' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || "_=:.;>-".contains(*c))
        .take(160)
        .collect()
}

pub fn settings_summary(profile: &ProfileDraft) -> Value {
    let short = |text: &str| text.chars().take(160).collect::<String>();
    json!({
        "province": short(&profile.province),
        "procedure": short(&profile.procedure),
        "subprocedure": profile.subprocedure.as_deref().map(short),
        "access_mode": profile.access_mode,
        "clave_method": profile.clave_method,
        "fill_mode": profile.fill_mode,
        "no_slots_mode": profile.no_slots_mode,
        "applicant_no_slots_mode": profile.applicant_no_slots_mode,
        "browser": profile.browser,
    })
}

/// Time between two tries to save a report that could not be saved.
pub const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

/// A report that is due to be saved. It is the finished report of the stop, built when the stop happened: the memory of
/// the search moves on, so a retry saves this same report and not a new one. It stays here until a save has succeeded
/// (the disk may be away for a while, the search may be paused meanwhile); a failed try only moves the next try by
/// `RETRY_DELAY`.
#[derive(Default)]
pub struct PendingReport {
    report: Option<Value>,
    next_try: Option<std::time::Instant>,
}
impl PendingReport {
    /// A stop wants this report saved; a newer one replaces a report that is still waiting.
    pub fn request(&mut self, report: Value) {
        self.report = Some(report);
    }
    /// Try the save when one is due and its delay is over. Returns the id of the saved report; a failure keeps it pending.
    pub fn try_save(&mut self, now: std::time::Instant, save: impl FnOnce(&Value) -> Option<String>) -> Option<String> {
        let report = self.report.as_ref()?;
        if self.next_try.is_some_and(|due| now < due) {
            return None;
        }
        match save(report) {
            Some(id) => {
                self.report = None;
                self.next_try = None;
                Some(id)
            }
            None => {
                self.next_try = Some(now + RETRY_DELAY);
                None
            }
        }
    }
}

#[derive(Default)]
pub struct SessionRecorder {
    rows: VecDeque<Value>,
    context: Value,
    last_page_key: Option<String>,
    stale_streak: u32,
}

impl SessionRecorder {
    /// The facts that stay the same for the whole session: the header row and the settings.
    pub fn set_context(&mut self, header: Value, settings: Value) {
        self.context = json!({"header": header, "settings": settings});
    }
    fn push(&mut self, row: Value) {
        if self.rows.len() == MAX_ROWS {
            self.rows.pop_front();
        }
        self.rows.push_back(row);
    }
    /// One observation. Returns `true` when the flow stopped because it cannot continue on its own (`Hold`) on a
    /// page it had not stopped on yet, which is when a report should be saved. Expected waits (CAPTCHA, a choice of
    /// office, a free date) are `Alert` or `Handoff` and are not stops to report.
    pub fn page(&mut self, at: &str, observation: &Observation, effects: &[FlowEffect], report: &Value) -> bool {
        let held = effects.contains(&FlowEffect::Hold);
        let effect_names = effects.iter().map(effect_name).collect::<Vec<_>>();
        let page = crate::session_timeline::page_row(observation.kind, observation.method, &observation.document);
        let key = format!("{page}{effect_names:?}{}", report["flow"]["phase"]);
        // The developer window shows the newest state even when the page row below is not repeated.
        live_update(|live| {
            live.insert("t".into(), json!(at));
            live.insert("page".into(), page["page"].clone());
            live.insert("variant".into(), json!(observation.variant));
            live.insert("effects".into(), json!(effect_names));
            live.insert("flow".into(), report["flow"].clone());
        });
        if self.last_page_key.as_deref() == Some(key.as_str()) {
            // The same stop seen again: keep the newest facts of that page, without a second signal.
            if let Some(row) = self.rows.iter_mut().rev().find(|row| row["kind"] == "page") {
                row["flow"] = report["flow"].clone();
                row["detail"] = json!({"facts": report["facts"], "page": report["page"]});
            }
            return false;
        }
        self.last_page_key = Some(key);
        self.push(json!({
            "t": at, "kind": "page", "page": page["page"], "method": page["method"], "doc": page["doc"],
            "variant": observation.variant, "operation_code": observation.operation_code,
            "effects": effect_names, "flow": report["flow"], "preparation": report["preparation"],
            "detail": {"facts": report["facts"], "page": report["page"]},
        }));
        held
    }
    /// A step the browser executed (`session_timeline::step_row` or `step_error_row`).
    pub fn step(&mut self, at: &str, row: Value) {
        // A step that did nothing (only waits, no receipt, no error) says nothing: it must not push the page out.
        let quiet = row["effects"].as_array().is_none_or(|effects| {
            effects.iter().all(|effect| matches!(effect.as_str(), Some("wait_until" | "reobserve" | "silence")))
        });
        let no_receipts = row["receipts"].as_array().is_none_or(Vec::is_empty);
        if quiet && no_receipts && row.get("error").is_none() {
            return;
        }
        let mut row = row;
        if let Some(detail) = row["detail"].as_str().map(stale_code) {
            row["detail"] = json!(detail);
        }
        // Where a field entry stopped: a fixed field and check name with counts, kept to plain code characters.
        if let Some(stopped) = row["entry_stopped"].as_str().map(stale_code) {
            row["entry_stopped"] = json!(stopped);
        }
        live_update(|live| {
            live.insert("last_step".into(), json!({"t": at, "effects": row["effects"], "error": row.get("error"), "detail": row.get("detail"), "entry_stopped": row.get("entry_stopped")}));
            if let Some(stopped) = row.get("entry_stopped") {
                live.insert("entry_stopped".into(), stopped.clone());
            }
        });
        row["t"] = json!(at);
        row["kind"] = json!("step");
        self.push(row);
    }
    /// The browser could not act on the page it had planned for. Returns `true` on the third one in a row.
    pub fn stale(&mut self, at: &str, reason: Option<&str>) -> bool {
        self.stale_streak += 1;
        live_update(|live| {
            live.insert("stale".into(), json!({"t": at, "reason": reason.map(stale_code), "streak": self.stale_streak}));
        });
        self.push(json!({"t": at, "kind": "stale_document", "reason": reason.map(stale_code), "streak": self.stale_streak}));
        self.stale_streak == 3
    }
    pub fn progressed(&mut self) {
        self.stale_streak = 0;
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    /// The report: environment, settings, why the search stopped and the newest events.
    pub fn report(&self, version: &str, now: &str, stop: Value) -> Value {
        let mut rows: Vec<Value> = self.rows.iter().cloned().collect();
        // Only page rows carry a detail block; a step's `detail` is its own reason and stays.
        let is_detailed = |row: &Value| row["kind"] == "page" && row.get("detail").is_some();
        let detailed = rows.iter().filter(|row| is_detailed(row)).count();
        let mut strip = detailed.saturating_sub(DETAILED_ROWS);
        for row in &mut rows {
            if strip > 0 && is_detailed(row) {
                row.as_object_mut().map(|object| object.remove("detail"));
                strip -= 1;
            }
        }
        let mut report = json!({
            "version": version,
            "time": now,
            "system": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "phase": "session_stop",
            "error_type": stop["reason"],
            "message": stop["summary"],
            "browser": {
                "version": self.context["header"]["firefox"],
                "webdriver_mode": self.context["header"]["webdriver_mode"],
                "certificate_cache": self.context["header"]["certificate_cache"],
            },
            "session": {
                "stop": stop,
                "settings": self.context["settings"],
                "header": self.context["header"],
                "recent": rows,
            },
        });
        while report.to_string().len() > MAX_BYTES {
            let Some(recent) = report["session"]["recent"].as_array_mut() else { break };
            if recent.is_empty() {
                break;
            }
            recent.remove(0);
        }
        report
    }
}

fn effect_name(effect: &FlowEffect) -> String {
    crate::session_timeline::effect_label(effect)
}

/// The newest state of the running search for the developer window: names, counts and codes only (the same as in a
/// report, without page facts). Written by the worker, read by the window.
static LIVE: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);
fn live_update(work: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    if let Ok(mut live) = LIVE.lock() {
        let state = live.get_or_insert_with(|| json!({}));
        if let Some(object) = state.as_object_mut() {
            work(object);
        }
    }
}
/// What the running search last saw and did (`null` before the first page of a search).
pub fn live() -> Value {
    LIVE.lock().ok().and_then(|live| live.clone()).unwrap_or(Value::Null)
}
thread_local! {
    /// The search worker is one thread for the whole run, so its memory needs no plumbing through the loop.
    static RECORDER: RefCell<SessionRecorder> = RefCell::new(SessionRecorder::default());
}
/// Forget the previous search (a new one starts on this thread).
pub fn begin() {
    RECORDER.with(|recorder| *recorder.borrow_mut() = SessionRecorder::default());
    if let Ok(mut live) = LIVE.lock() {
        *live = None;
    }
}
pub fn with<T>(work: impl FnOnce(&mut SessionRecorder) -> T) -> T {
    RECORDER.with(|recorder| work(&mut recorder.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ceh_core::model::{DocumentId, PageKind, RequestMethod};

    fn profile() -> ProfileDraft {
        serde_json::from_value(json!({
            "id": "p1", "label": "Maria Garcia Lopez", "enabled": true,
            "procedure": "POLICIA-TOMA DE HUELLAS (EXPEDICIÓN DE TARJETA) Y RENOVACIÓN DE TARJETA DE LARGA DURACIÓN",
            "province": "Valencia", "office": "CNP COMISARIA SECRET OFFICE",
            "document_type": "nie", "document_number": "X1234567L", "full_name": "Maria Garcia Lopez",
            "nationality": "RUSIA", "phone": "600123456", "email": "maria@example.com",
            "access_mode": "certificate", "clave_method": "movil", "fill_mode": "copy",
            "no_slots_mode": "reload", "applicant_no_slots_mode": "continue", "subprocedure": null,
            "timezone": "Europe/Madrid", "browser_session": "persistent"
        }))
        .unwrap()
    }
    fn observation(kind: PageKind, document: &str) -> Observation {
        Observation {
            document: DocumentId::new(document).unwrap(),
            kind,
            method: RequestMethod::Get,
            operation_code: false,
            variant: "ab12cd34".into(),
        }
    }
    fn detail() -> Value {
        json!({"flow": {"phase": "held", "hold_reason": "no_handler_for_page"}, "facts": {"office_count": 0}, "page": {"endpoint": "acInfo"}})
    }

    #[test]
    fn the_settings_keep_the_province_and_the_service_but_no_identity() {
        let settings = settings_summary(&profile());
        assert_eq!(settings["province"], "Valencia");
        assert!(settings["procedure"].as_str().unwrap().starts_with("POLICIA-TOMA DE HUELLAS"));
        assert_eq!(settings["access_mode"], "certificate");
        assert_eq!(settings["no_slots_mode"], "reload");
        let text = settings.to_string();
        for private in ["Maria", "Garcia", "X1234567L", "600123456", "example.com", "SECRET", "RUSIA", "p1"] {
            assert!(!text.contains(private), "{private} leaked into {text}");
        }
    }

    #[test]
    fn a_hold_on_a_new_page_is_reported_once() {
        let mut recorder = SessionRecorder::default();
        let o = observation(PageKind::Office, "doc-1");
        assert!(recorder.page("t1", &o, &[FlowEffect::Hold], &detail()));
        assert!(!recorder.page("t2", &o, &[FlowEffect::Hold], &detail()), "the same stop is not reported again");
        assert!(!recorder.page("t3", &observation(PageKind::Province, "doc-2"), &[FlowEffect::Silence], &detail()));
        assert!(recorder.page("t4", &observation(PageKind::Office, "doc-3"), &[FlowEffect::Hold], &detail()));
    }

    #[test]
    fn three_unusable_documents_in_a_row_ask_for_a_report() {
        let mut recorder = SessionRecorder::default();
        assert!(!recorder.stale("t1", Some("target_changed_after_scroll")));
        assert!(!recorder.stale("t2", None));
        assert!(recorder.stale("t3", None));
        recorder.progressed();
        assert!(!recorder.stale("t4", None));
    }

    #[test]
    fn the_report_names_the_stop_and_stays_within_the_store_limit() {
        let mut recorder = SessionRecorder::default();
        recorder.set_context(json!({"firefox": "Chrome 157", "webdriver_mode": "cdp", "certificate_cache": "ready"}), settings_summary(&profile()));
        let big = json!({"flow": {"phase": "waiting"}, "facts": {}, "page": {"visible_selectors": vec!["#a_long_selector_name_for_a_probe"; 64]}});
        for i in 0..200 {
            recorder.page("t", &observation(PageKind::Province, &format!("doc-{i}")), &[FlowEffect::Silence], &big);
        }
        let report = recorder.report("0.4.3", "2026-11-18T10:00:00Z", json!({"reason": "no_handler_for_page", "summary": "x"}));
        assert_eq!(report["phase"], "session_stop");
        assert_eq!(report["error_type"], "no_handler_for_page");
        assert_eq!(report["session"]["settings"]["province"], "Valencia");
        let recent = report["session"]["recent"].as_array().unwrap();
        assert!(recent.len() <= MAX_ROWS);
        assert_eq!(recent.iter().filter(|row| row.get("detail").is_some()).count(), DETAILED_ROWS);
        assert!(report.to_string().len() <= MAX_BYTES);
    }

    #[test]
    fn no_identity_reaches_the_report() {
        let mut recorder = SessionRecorder::default();
        recorder.set_context(json!({"firefox": "Chrome 157"}), settings_summary(&profile()));
        recorder.page("t", &observation(PageKind::Province, "document-with-an-id"), &[FlowEffect::Hold], &detail());
        let text = recorder.report("0.4.3", "now", json!({"reason": "r", "summary": "s"})).to_string();
        for private in ["Maria", "X1234567L", "600123456", "example.com", "SECRET", "document-with-an-id"] {
            assert!(!text.contains(private), "{private} leaked into {text}");
        }
    }

    #[test]
    fn step_reasons_keep_their_detail_when_old_pages_lose_theirs() {
        let mut recorder = SessionRecorder::default();
        for i in 0..6 {
            recorder.page("t", &observation(PageKind::Province, &format!("doc-{i}")), &[FlowEffect::Silence], &detail());
            if i == 1 {
                recorder.stale("t", Some("target_changed_after_scroll"));
                recorder.step("t", json!({"effects": ["activate:province"], "detail": "target_changed_after_scroll", "error": "stale_document"}));
            }
        }
        let report = recorder.report("0.4.3", "now", json!({"reason": "r", "summary": "s"}));
        let recent = report["session"]["recent"].as_array().unwrap();
        let page_details = recent.iter().filter(|row| row["kind"] == "page" && row.get("detail").is_some()).count();
        assert_eq!(page_details, DETAILED_ROWS);
        let step = recent.iter().find(|row| row["kind"] == "step").unwrap();
        assert_eq!(step["detail"], "target_changed_after_scroll", "a step's reason is not a page detail");
    }

    #[test]
    fn quiet_steps_do_not_push_the_last_page_out() {
        let mut recorder = SessionRecorder::default();
        recorder.page("t", &observation(PageKind::Office, "doc-1"), &[FlowEffect::Hold], &detail());
        for _ in 0..100 {
            recorder.step("t", json!({"effects": [], "delay_ms": null, "elapsed_ms": 1, "receipts": []}));
            recorder.step("t", json!({"effects": ["wait_until"], "elapsed_ms": 1, "receipts": []}));
        }
        recorder.step("t", json!({"effects": ["reload"], "elapsed_ms": 5, "receipts": []}));
        let report = recorder.report("0.4.3", "now", json!({"reason": "r", "summary": "s"}));
        let recent = report["session"]["recent"].as_array().unwrap();
        assert!(recent.iter().any(|row| row["kind"] == "page"), "the page the search stopped on is kept");
        assert!(recent.iter().any(|row| row["kind"] == "step" && row["effects"][0] == "reload"), "a real action is kept");
        assert!(recent.len() < 5, "{recent:?}");
    }

    #[test]
    fn the_same_stop_refreshes_its_page_facts_without_a_second_signal() {
        let mut recorder = SessionRecorder::default();
        let o = observation(PageKind::Office, "doc-1");
        let first = json!({"flow": {"phase": "held"}, "facts": {"office_count": 0}, "page": {}});
        let later = json!({"flow": {"phase": "held"}, "facts": {"office_count": 2}, "page": {}});
        assert!(recorder.page("t1", &o, &[FlowEffect::Hold], &first));
        assert!(!recorder.page("t2", &o, &[FlowEffect::Hold], &later));
        let report = recorder.report("0.4.3", "now", json!({"reason": "r", "summary": "s"}));
        let recent = report["session"]["recent"].as_array().unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0]["detail"]["facts"]["office_count"], 2);
    }

    #[test]
    fn a_report_that_cannot_be_saved_stays_pending_and_is_retried_after_a_delay() {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let mut pending = PendingReport::default();
        assert!(pending.try_save(start, |_| panic!("nothing is pending")).is_none());
        pending.request(json!({"stop": "no_handler_for_page"}));
        let mut tries = 0;
        // The disk is gone for a long time (many more tries than any fixed limit): the report is never dropped.
        for second in 0..600u64 {
            let now = start + Duration::from_secs(second);
            assert!(pending.try_save(now, |_| { tries += 1; None }).is_none());
        }
        assert!((100..=130).contains(&tries), "a bounded delay between tries, got {tries}");
        // The disk is back (also while the search is paused): the same report is saved once.
        let saved = pending.try_save(start + Duration::from_secs(700), |report| {
            assert_eq!(report["stop"], "no_handler_for_page");
            Some("20261118T100000-00000000".into())
        });
        assert_eq!(saved.as_deref(), Some("20261118T100000-00000000"));
        assert!(pending.try_save(start + Duration::from_secs(800), |_| panic!("saved already")).is_none());
    }

    #[test]
    fn a_failed_save_is_not_retried_before_the_delay_and_a_newer_request_replaces_the_pending_one() {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let mut pending = PendingReport::default();
        pending.request(json!({"n": "first"}));
        assert!(pending.try_save(start, |_| None).is_none());
        assert!(pending.try_save(start + Duration::from_millis(500), |_| panic!("too early")).is_none());
        pending.request(json!({"n": "second"}));
        let saved = pending.try_save(start + RETRY_DELAY, |report| {
            assert_eq!(report["n"], "second");
            Some("id".into())
        });
        assert_eq!(saved.as_deref(), Some("id"));
    }

    #[test]
    fn a_retried_report_is_the_one_built_at_the_stop_not_a_new_one() {
        use std::time::Instant;
        let mut recorder = SessionRecorder::default();
        recorder.set_context(json!({"firefox": "Chrome 157"}), settings_summary(&profile()));
        recorder.page("t1", &observation(PageKind::Office, "stopped-here"), &[FlowEffect::Hold], &detail());
        // The report is built when the stop happens and kept as it is.
        let mut pending = PendingReport::default();
        pending.request(recorder.report("0.4.3", "now", json!({"reason": "no_handler_for_page", "summary": "s"})));
        let start = Instant::now();
        assert!(pending.try_save(start, |_| None).is_none());
        // The person moves on and the memory fills with later pages, so that the stop falls out of it.
        for i in 0..100 {
            recorder.page("t2", &observation(PageKind::Province, &format!("later-{i}")), &[FlowEffect::Silence], &detail());
        }
        let mut saved_report = None;
        pending.try_save(start + RETRY_DELAY, |report| {
            saved_report = Some(report.clone());
            Some("id".into())
        });
        let text = saved_report.expect("the report was saved").to_string();
        assert!(text.contains("no_handler_for_page") && text.contains("\"page\":\"office\""), "the stop is still in it: {text}");
        assert!(!text.contains("\"page\":\"province\""), "later pages are not in it: {text}");
    }

    #[test]
    fn page_text_in_a_stale_reason_never_reaches_the_report() {
        let reason = "page changed: text_equal=false visible_equal=true endpoint_equal=true len=120->118 first_diff=40 before=\"Maria Garcia NIE X1234567L\" after=\"otro texto\"";
        let mut recorder = SessionRecorder::default();
        recorder.page("t0", &observation(PageKind::Expired, "doc-1"), &[FlowEffect::Silence], &detail());
        recorder.stale("t1", Some(reason));
        recorder.step("t2", json!({"effects": ["activate:province"], "error": "stale_document", "detail": reason}));
        let text = recorder.report("0.4.3", "now", json!({"reason": "r", "summary": "s"})).to_string();
        for private in ["Maria", "Garcia", "X1234567L", "otro texto", "before=", "after="] {
            assert!(!text.contains(private), "{private} leaked into {text}");
        }
        assert!(text.contains("text_equal=false") && text.contains("first_diff=40"), "the structured part stays: {text}");
    }
}
