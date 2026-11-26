//! The CDP wire against a scripted fake browser (no Chrome): a command that outlives its timeout, because a
//! native dialog (certificate, keychain) holds the browser, must not poison the connection; a slow browser
//! finishes a click gesture, and a gesture cut off before the button went down says nothing was clicked.
use ceh_browser::bidi::{BidiConnection, ProtocolError};
use serde_json::{json, Value};
use std::{net::TcpListener, time::Duration};

/// A fake browser that holds each request for `delay(request)` before it answers.
fn fake_browser(delay: impl Fn(&Value) -> Duration + Send + 'static) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        while let Ok(message) = socket.read() {
            let Ok(text) = message.into_text() else { break };
            let request: Value = serde_json::from_str(&text).unwrap();
            std::thread::sleep(delay(&request));
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => json!({"targetInfos":[{"targetId":"T","type":"page"}]}),
                "Target.attachToTarget" => json!({"sessionId":"S"}),
                "Page.createIsolatedWorld" => json!({"executionContextId":1}),
                "Runtime.evaluate" => json!({"result":{"type":"number","value":2}}),
                _ => json!({}),
            };
            if socket.send(tungstenite::Message::Text(json!({"id":request["id"],"result":result}).to_string().into())).is_err() {
                break;
            }
        }
    });
    port
}
fn connect(port: u16) -> BidiConnection {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    BidiConnection::connect_cdp(address, "/devtools/browser/fake", Duration::from_millis(300)).unwrap()
}
/// A click as `trusted_input` sends it: cursor moves, a press and a release.
fn click(moves: usize) -> Value {
    let mut actions: Vec<Value> = (0..moves).map(|i| json!({"type":"pointerMove","x":i,"y":i,"duration":1,"origin":"pointer"})).collect();
    actions.push(json!({"type":"pointerDown","button":0}));
    actions.push(json!({"type":"pointerUp","button":0}));
    json!({"context":"T","actions":[{"type":"pointer","id":"p","parameters":{"pointerType":"mouse"},"actions":actions}]})
}
fn mouse(request: &Value) -> &str {
    if request["method"] == "Input.dispatchMouseEvent" { request["params"]["type"].as_str().unwrap_or("") } else { "" }
}

#[test]
fn a_command_held_past_its_timeout_fails_alone_and_the_connection_goes_on() {
    let first = std::sync::atomic::AtomicBool::new(true);
    let port = fake_browser(move |request| {
        if request["method"] == "Runtime.evaluate" && first.swap(false, std::sync::atomic::Ordering::SeqCst) {
            Duration::from_millis(450)
        } else {
            Duration::ZERO
        }
    });
    let mut connection = connect(port);
    let params = json!({"expression":"1+1","target":{"context":"T"},"awaitPromise":false});
    assert!(connection.call("script.evaluate", params.clone()).is_err(), "the held command times out");
    assert!(connection.available(), "a slow browser is not a broken connection");
    // The late reply of the first command arrives while the next one waits: it is not mistaken for its answer.
    let again = connection.call("script.evaluate", params).unwrap();
    assert_eq!(again["result"], json!({"type":"number","value":2}));
}

#[test]
fn a_slow_browser_finishes_a_click_that_takes_longer_than_one_command_timeout() {
    // Eight moves of 100 ms each: the gesture takes about 1 s against a 300 ms command timeout.
    let port = fake_browser(|request| if mouse(request) == "mouseMoved" { Duration::from_millis(100) } else { Duration::ZERO });
    let mut connection = connect(port);
    assert!(connection.call("input.performActions", click(8)).is_ok());
}

#[test]
fn a_click_cut_off_before_the_button_went_down_is_reported_as_not_performed() {
    let port = fake_browser(|request| if mouse(request) == "mouseMoved" { Duration::from_millis(450) } else { Duration::ZERO });
    let mut connection = connect(port);
    assert_eq!(connection.call("input.performActions", click(3)), Err(ProtocolError::NotPerformed));
    assert!(connection.available(), "nothing was clicked and the connection goes on");
}

#[test]
fn a_click_held_after_the_button_went_down_stays_ambiguous() {
    let port = fake_browser(|request| if mouse(request) == "mouseReleased" { Duration::from_millis(450) } else { Duration::ZERO });
    let mut connection = connect(port);
    assert_eq!(connection.call("input.performActions", click(3)), Err(ProtocolError::Rejected));
}
