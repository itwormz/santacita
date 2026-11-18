//! Names, flags and counts that say where a search is and why it stopped, for the report a person can copy.
//! Never page text, typed values, ids or cookies: every string here is a fixed code or a short token.
use ceh_core::{flow::{FlowDiagnostics, PageFacts}, recognition::PageSnapshot};
use serde_json::{Value, json};

/// A short plain token (letters, digits, underscore): anything else the page might put there is dropped.
fn token(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect()
}
fn name<T: std::fmt::Debug>(value: T) -> String {
    token(&format!("{value:?}")).to_ascii_lowercase()
}

/// The last path segment of the page address is page content when the site is not the one we know (an identifier can
/// sit there): only the exact names of the site's own steps are reported, everything else is `unknown`.
fn endpoint_code(raw: &str) -> &'static str {
    ["acCitar", "acEntrada", "acInfo", "acOpcDirect", "acValidarEntrada", "salirInicio", "index"]
        .into_iter()
        .find(|known| *known == raw)
        .unwrap_or("unknown")
}
/// The page as the recognizer saw it: which probe selectors were visible, the shape counts, frame statuses.
/// The probe list is fixed in the page script, so selector names are not page content.
pub fn page_state(snapshot: &PageSnapshot) -> Value {
    json!({
        "endpoint": endpoint_code(&snapshot.endpoint),
        "official": snapshot.official,
        "applicant_editable": snapshot.applicant_editable,
        "fill_mode": token(&snapshot.fill_mode),
        "last_action": snapshot.last_action.as_deref().map(token),
        "pending_action": snapshot.pending_action.as_deref().map(token),
        "visible_selectors": snapshot.visible.iter().filter(|s| s.len() <= 80).take(64).collect::<Vec<_>>(),
        "frames": snapshot.frames.iter().take(8).map(|frame| json!({
            "status": frame.status,
            "response_matches": frame.response_matches,
            "words": frame.text.split_whitespace().count(),
        })).collect::<Vec<_>>(),
        "shape": {
            "forms": snapshot.shape.forms,
            "selects": snapshot.shape.selects,
            "buttons": snapshot.shape.buttons,
            "inputs": snapshot.shape.inputs,
        },
    })
}
/// The facts the flow decided on: flags and counts, no values.
pub fn facts_state(facts: &PageFacts) -> Value {
    json!({
        "office_count": facts.office_count,
        "appointment_scheduled": facts.appointment_ready_at.is_some(),
        "confirm_appointment_round": facts.confirm_appointment_round,
        "cookie_accept_available": facts.cookie_accept_available,
        "authentication": name(facts.authentication),
        "certificate_method_available": facts.certificate_method_available,
        "clave_method": name(facts.clave_method),
        "clave_method_available": facts.clave_method_available,
        "clave_requires_attention": facts.clave_requires_attention,
        "clave_input_started": facts.clave_input_started,
        "recovery": {
            "official_site": facts.recovery.official_site,
            "endpoint": name(facts.recovery.endpoint),
            "last_action": facts.recovery.last_action.map(name),
            "pending_action": facts.recovery.pending_action.map(name),
            "own_control": facts.recovery.own_control,
            "certificate_validation_error": facts.recovery.certificate_validation_error,
            "immediate_expiry_control": facts.recovery.immediate_expiry_control,
        },
    })
}
/// Everything the driver adds to one observation for the report.
pub fn driver_state(flow: FlowDiagnostics, facts: &PageFacts, snapshot: &PageSnapshot) -> Value {
    json!({"flow": flow, "facts": facts_state(facts), "page": page_state(snapshot)})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PageSnapshot {
        serde_json::from_value(json!({
            "visible": ["body", "#form", "#btnAceptar"],
            "frames": [{"text": "Hola Juan Pérez NIE X1234567L 600123456 juan@example.com", "status": 200}],
            "official": true,
            "endpoint": "acInfo",
            "applicant_editable": false,
            "fill_mode": "copy",
            "button_label": "Pérez Juan secret label",
            "last_action": "province",
            "pending_action": null,
            "shape": {"forms": 1, "selects": 2, "buttons": 3, "inputs": [0, 1, 2, 3, 4, 5, 6]}
        }))
        .unwrap()
    }

    #[test]
    fn the_page_state_names_what_is_on_the_page_without_its_text() {
        let state = page_state(&snapshot());
        assert_eq!(state["endpoint"], "acInfo");
        assert_eq!(state["visible_selectors"], json!(["body", "#form", "#btnAceptar"]));
        assert_eq!(state["frames"][0]["status"], 200);
        assert_eq!(state["frames"][0]["words"], 7);
        assert_eq!(state["shape"]["selects"], 2);
        assert_eq!(state["fill_mode"], "copy");
        assert_eq!(state["applicant_editable"], false);
        let text = state.to_string();
        for private in ["Juan", "Pérez", "X1234567L", "600123456", "example.com", "secret"] {
            assert!(!text.contains(private), "{private} leaked into {text}");
        }
    }

    #[test]
    fn an_endpoint_the_app_does_not_know_is_never_copied() {
        for raw in ["acInfo?nie=X1234567L&name=Juan Pérez", "X1234567L", "acInfo/../X1234567L", "ACINFO", ""] {
            let mut shot = snapshot();
            shot.endpoint = raw.to_owned();
            let state = page_state(&shot);
            assert_eq!(state["endpoint"], "unknown", "{raw}");
            assert!(!state.to_string().contains("X1234567L"));
        }
        for known in ["acCitar", "acEntrada", "acInfo", "acOpcDirect", "acValidarEntrada", "salirInicio", "index"] {
            let mut shot = snapshot();
            shot.endpoint = known.to_owned();
            assert_eq!(page_state(&shot)["endpoint"], known);
        }
    }

    #[test]
    fn the_facts_state_has_flags_and_counts_only() {
        let facts = PageFacts { office_count: 3, cookie_accept_available: true, ..Default::default() };
        let state = facts_state(&facts);
        assert_eq!(state["office_count"], 3);
        assert_eq!(state["cookie_accept_available"], true);
        assert_eq!(state["appointment_scheduled"], false);
        assert_eq!(state["recovery"]["own_control"], false);
    }
}
