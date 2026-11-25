//! Small local startup-failure reports (Python `app/desktop/startup_report.py`): never page content or
//! profile data, scrubbed again when read and when copied, at most ten kept.
use ceh_services::diagnostics::{Sanitizer, StartupReports};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn directory(root: &Path) -> PathBuf {
    root.join("data/startup-errors")
}
fn reports(root: &Path, home: &str) -> StartupReports {
    StartupReports::new(&directory(root), Sanitizer::default(), home)
}
/// Phases the window distinguishes: the browser could not start, or the search failed afterwards.
pub fn phase_of(code: &str) -> &'static str {
    match code {
        "firefox_not_installed" | "browser_profile_unavailable" | "browser_start_failed" | "browser_certificate_cache_failed" => "browser_launch",
        _ => "worker",
    }
}
/// What is known about the browser of a failed start: its family, whether it was found, and why it did not start.
pub struct StartFacts {
    pub family: &'static str,
    pub found: bool,
    pub reason: Option<&'static str>,
    /// How far and how long the start got, and how the browser ended: numbers and a version only.
    pub detail: Option<StartDetail>,
}
/// Numbers and a version of a start that failed (the browser crate's own `StartDetail`, copied where the report is built).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartDetail {
    pub browser_version: String,
    pub elapsed_ms: u64,
    pub exit_code: Option<i32>,
    pub stderr_tail: Vec<String>,
}
thread_local! {
    static START_FAILURE: std::cell::Cell<Option<(&'static str, &'static str)>> = const { std::cell::Cell::new(None) };
}
/// The worker notes which browser failed to start and why; the report written after the worker returns reads it.
pub fn note_start_failure(family: &'static str, reason: &'static str) {
    START_FAILURE.with(|cell| cell.set(Some((family, reason))));
}
/// What the current thread noted, once.
pub fn take_start_failure() -> Option<(&'static str, &'static str)> {
    START_FAILURE.with(|cell| cell.take())
}
/// Save a report for a failed search start and return its id; `None` when nothing could be written.
pub fn save_failure(root: &Path, home: &str, code: &str, firefox_found: bool, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    save_failure_with(root, home, code, &StartFacts { family: "firefox", found: firefox_found, reason: None, detail: None }, now)
}
pub fn save_failure_with(root: &Path, home: &str, code: &str, facts: &StartFacts, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let mut browser = json!({"family": facts.family, "exists": facts.found});
    if let Some(reason) = facts.reason {
        browser["start_failure"] = json!(reason);
    }
    if let Some(detail) = &facts.detail {
        let mut object = json!({"browser_version": detail.browser_version, "elapsed_ms": detail.elapsed_ms});
        if let Some(code) = detail.exit_code {
            // Windows reports a failed start as a status such as 0xC0000135: the hexadecimal form is the one to look up.
            object["exit_code"] = json!(code);
            object["exit_code_hex"] = json!(format!("{:#010x}", code as u32));
        }
        if !detail.stderr_tail.is_empty() {
            object["stderr_tail"] = json!(detail.stderr_tail);
        }
        browser["start_detail"] = object;
    }
    let report = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "time": now.to_rfc3339(),
        "system": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "frozen": false,
        "executable": std::env::current_exe().ok().map(|p| p.display().to_string()).unwrap_or_default(),
        "phase": phase_of(code),
        "browser": browser,
        "bundle_checks": {},
        "error_type": code,
        "message": code,
        "traceback": [],
    });
    reports(root, home).save(report, now).ok()
}
/// The newest saved report, for the About window; `None` when nothing was ever saved.
pub fn latest_id(root: &Path, home: &str) -> Option<String> {
    reports(root, home).latest()
}
/// Save the report of a stop that waits for a person (`session_report`); the store keeps its fixed fields only.
pub fn save_session_stop(root: &Path, home: &str, report: Value, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    reports(root, home).save(report, now).ok()
}
/// The text copied to the clipboard for report `id`, scrubbed again; `Err` when it is gone or invalid.
pub fn clipboard_text(root: &Path, home: &str, id: &str) -> Result<String, &'static str> {
    let store = reports(root, home);
    let report: Value = store.read(id).map_err(|_| "report_unavailable")?.ok_or("report_unavailable")?;
    store.clipboard(id, report).map_err(|_| "report_unavailable")
}
