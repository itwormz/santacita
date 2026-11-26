use ceh_browser::{bidi::{BidiConnection, ProtocolError}, learning::{ReadFailure, parse_page, read_page}};
use serde_json::{Value, json};
use std::{net::TcpListener, path::Path, thread, time::Duration};
use tungstenite::Message;

fn madrid() -> Value {
    json!({
        "path": "/icpplustiem/selectSede",
        "load": "1000.5",
        "title": "Proceso automático para la solicitud de cita previa",
        "headings": ["Solicitud de cita previa"],
        "text": ["Seleccione la oficina y el trámite"],
        "forms": [{"id": "", "name": "", "action": "acInfo", "method": "post"}],
        "controls": [
            {"tag": "select", "kind": "select", "id": "sede", "name": "sede", "label": "Oficina", "form": "acInfo", "disabled": false,
             "options": [
                {"value": "99", "label": "Cualquier oficina", "selected": true},
                {"value": "7", "label": "CNP AVDA POBLADOS, Avda. de los Poblados, S/N, MADRID", "selected": false},
                {"value": "59", "label": "CNP COLMENAR VIEJO, Carlos Aragón Cancela, 5, Colmenar Viejo", "selected": false}]},
            {"tag": "select", "kind": "select", "id": "tramiteGrupo[0]", "name": "tramiteGrupo[0]", "label": "Trámite", "form": "acInfo", "disabled": false,
             "options": [{"value": "4036", "label": "POLICIA - RECOGIDA DE TARJETA DE IDENTIDAD DE EXTRANJERO (TIE)", "selected": false}]},
            {"tag": "input", "kind": "hidden", "id": "", "name": "prov_selecc", "label": "", "form": "acInfo", "disabled": false, "options": []}
        ]
    })
}

#[test]
fn a_form_shape_parses_with_every_select_option() {
    let page = parse_page(&madrid().to_string()).unwrap();
    assert_eq!(page.path, "/icpplustiem/selectSede");
    let ids: Vec<_> = page.controls.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["sede", "tramiteGrupo[0]", ""]);
    assert_eq!(page.controls[0].options.len(), 3);
    assert_eq!(page.forms[0].action, "acInfo");
}

#[test]
fn a_control_carrying_a_value_key_is_rejected() {
    let mut raw = madrid();
    raw["controls"][2]["value"] = json!("X0000000T");
    assert!(parse_page(&raw.to_string()).is_none());
}

#[test]
fn a_page_over_the_limits_is_rejected() {
    let mut raw = madrid();
    let control = raw["controls"][2].clone();
    raw["controls"] = Value::Array(vec![control; 401]);
    assert!(parse_page(&raw.to_string()).is_none());
    assert!(parse_page("not json").is_none());
}

#[test]
fn the_fingerprint_changes_when_an_option_is_added_and_not_when_nothing_changes() {
    let first = parse_page(&madrid().to_string()).unwrap();
    let same = parse_page(&madrid().to_string()).unwrap();
    assert_eq!(first.fingerprint(), same.fingerprint());
    let mut raw = madrid();
    raw["controls"][1]["options"].as_array_mut().unwrap().push(json!({"value": "4010", "label": "TOMA DE HUELLAS", "selected": false}));
    assert_ne!(first.fingerprint(), parse_page(&raw.to_string()).unwrap().fingerprint());
}

#[test]
fn the_fingerprint_ignores_the_load_stamp_so_a_refresh_is_told_apart_only_by_it() {
    let first = parse_page(&madrid().to_string()).unwrap();
    let mut raw = madrid();
    raw["load"] = json!("2000.25");
    let refreshed = parse_page(&raw.to_string()).unwrap();
    assert_eq!(first.fingerprint(), refreshed.fingerprint());
    assert_ne!(first.load, refreshed.load);
}

fn script() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/learning_snapshot.js")).unwrap()
}

#[test]
fn the_learning_script_never_reads_an_editable_value() {
    let code = script();
    // The only permitted `.value` read is an option's, marked at the end of its line.
    for line in code.lines().filter(|l| l.contains(".value")) {
        assert!(line.trim_end().ends_with("// option-value"), "unmarked value read: {line}");
    }
    for forbidden in ["defaultValue", "innerText", "textContent", "innerHTML", "outerHTML", "cookie", "localStorage", "sessionStorage"] {
        // `textContent` is read only for headings and labels through the `plain` helper.
        let allowed = if forbidden == "textContent" { 1 } else { 0 };
        assert!(code.matches(forbidden).count() <= allowed, "{forbidden} appears in the learning script");
    }
}

#[test]
fn reading_a_page_sends_the_script_to_the_given_target_and_parses_the_answer() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let answer = madrid().to_string();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: Value = serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        assert_eq!(request["method"], "script.callFunction");
        assert_eq!(request["params"]["target"], json!({"context": "ctx", "sandbox": "ceh"}));
        assert_eq!(request["params"]["awaitPromise"], false);
        socket.send(Message::Text(json!({"id": request["id"], "type": "success", "result": {"type": "success", "realm": "document-realm", "result": {"type": "string", "value": answer}}}).to_string().into())).unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    let page = read_page(&mut connection, &json!({"context": "ctx", "sandbox": "ceh"})).unwrap();
    assert_eq!(page.controls.len(), 3);
    server.join().unwrap();
}

fn answer_with(value: Value) -> Result<ceh_browser::learning::LearningPage, ReadFailure> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: Value = serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        socket.send(Message::Text(json!({"id": request["id"], "type": "success", "result": {"type": "success", "realm": "r", "result": value}}).to_string().into())).unwrap();
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    let result = read_page(&mut connection, &json!({"context": "ctx"}));
    server.join().unwrap();
    result
}

#[test]
fn a_page_the_script_declines_to_read_is_reported_as_empty() {
    assert_eq!(answer_with(json!({"type": "null"})).unwrap_err(), ReadFailure::Empty);
}

#[test]
fn an_answer_that_is_not_the_structure_is_reported_as_invalid() {
    assert_eq!(answer_with(json!({"type": "string", "value": "{\"path\":\"/x\",\"value\":\"secret\"}"})).unwrap_err(), ReadFailure::Invalid);
}

#[test]
fn a_call_that_the_browser_never_answers_is_reported_as_a_call_failure() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let _ = socket.read();
        // Never answers: the socket is dropped.
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(2)).unwrap();
    assert_eq!(read_page(&mut connection, &json!({"context": "ctx"})).unwrap_err(), ReadFailure::Call(ProtocolError::Unavailable));
    server.join().unwrap();
}

/// A page that was being loaded while nobody read the socket: hundreds of events wait before the answer.
fn read_behind_a_backlog(drop_on_overflow: bool) -> Result<ceh_browser::learning::LearningPage, ReadFailure> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let answer = madrid().to_string();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let request: Value = serde_json::from_str(&socket.read().unwrap().into_text().unwrap()).unwrap();
        for n in 0..300 {
            let event = json!({"type": "event", "method": "network.responseCompleted", "params": {"n": n}});
            if socket.send(Message::Text(event.to_string().into())).is_err() {
                return;
            }
        }
        let _ = socket.send(Message::Text(json!({"id": request["id"], "type": "success", "result": {"type": "success", "realm": "r", "result": {"type": "string", "value": answer}}}).to_string().into()));
    });
    let mut connection = BidiConnection::connect(address, Duration::from_secs(5)).unwrap();
    connection.set_drop_events_on_overflow(drop_on_overflow);
    let result = read_page(&mut connection, &json!({"context": "ctx"}));
    let _ = server.join();
    result
}

#[test]
fn a_backlog_of_events_ends_the_connection_unless_the_reader_drops_them() {
    assert_eq!(read_behind_a_backlog(false).unwrap_err(), ReadFailure::Call(ProtocolError::EventOverflow));
    assert!(read_behind_a_backlog(true).is_ok());
}
