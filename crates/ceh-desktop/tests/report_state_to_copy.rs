//! The page state the browser adapter adds to a report must reach the copied text without any page-controlled value.
#![cfg(feature = "shell")]
use ceh_desktop::startup_report::{clipboard_text, save_session_stop};
use serde_json::json;

#[test]
fn an_identifier_in_the_page_address_never_reaches_the_copied_report() {
    let snapshot: ceh_core::recognition::PageSnapshot = serde_json::from_value(json!({
        "visible": ["body", "#form"],
        "frames": [{"text": "Maria Garcia X1234567L", "status": 200}],
        "endpoint": "X1234567L",
        "last_action": "province",
        "shape": {"forms": 1, "selects": 1, "buttons": 1, "inputs": [0, 0, 0, 0, 0, 0, 0]}
    }))
    .unwrap();
    let page = ceh_browser::report_state::page_state(&snapshot);
    let report = json!({
        "version": "0.4.3", "phase": "session_stop", "error_type": "x", "message": "m",
        "session": {"stop": {"reason": "r"}, "recent": [{"kind": "page", "page": "province", "detail": {"page": page}}]},
    });
    let root = tempfile::tempdir().unwrap();
    let at = chrono::DateTime::parse_from_rfc3339("2026-11-18T10:00:00Z").unwrap().to_utc();
    let id = save_session_stop(root.path(), "/Users/Fictional", report, at).unwrap();
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    assert!(text.contains("\"endpoint\": \"unknown\""), "{text}");
    assert!(text.contains("#form") && text.contains("province"));
    for private in ["X1234567L", "Maria", "Garcia"] {
        assert!(!text.contains(private), "{private} leaked into {text}");
    }
}
