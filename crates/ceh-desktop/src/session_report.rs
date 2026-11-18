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
        if self.last_page_key.as_deref() == Some(key.as_str()) {
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
        let mut row = row;
        row["t"] = json!(at);
        row["kind"] = json!("step");
        self.push(row);
    }
    /// The browser could not act on the page it had planned for. Returns `true` on the third one in a row.
    pub fn stale(&mut self, at: &str, reason: Option<&str>) -> bool {
        self.stale_streak += 1;
        self.push(json!({"t": at, "kind": "stale_document", "reason": reason, "streak": self.stale_streak}));
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
        let detailed = rows.iter().filter(|row| row.get("detail").is_some()).count();
        let mut strip = detailed.saturating_sub(DETAILED_ROWS);
        for row in &mut rows {
            if strip > 0 && row.get("detail").is_some() {
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

thread_local! {
    /// The search worker is one thread for the whole run, so its memory needs no plumbing through the loop.
    static RECORDER: RefCell<SessionRecorder> = RefCell::new(SessionRecorder::default());
}
/// Forget the previous search (a new one starts on this thread).
pub fn begin() {
    RECORDER.with(|recorder| *recorder.borrow_mut() = SessionRecorder::default());
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
}
