//! A panic on the window's own thread aborts the whole app without any message a person could read, so the
//! place of the panic is kept for the next start (one test per process: the panic hook is global).
use ceh_desktop::crash_note::{class_of, install, report_previous, short_location};
use ceh_desktop::startup_report::{clipboard_text, latest_id};

fn at() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-11-20T10:00:00Z").unwrap().to_utc()
}

#[test]
fn a_panic_leaves_its_place_for_the_next_start_and_nothing_else() {
    // The message of a panic is sorted into a fixed class; the text itself is never kept.
    assert_eq!(class_of("called `Option::unwrap()` on a `None` value"), "none_unwrap");
    assert_eq!(class_of("called `Result::unwrap()` on an `Err` value: Os { code: 2, message: \"/Users/Fictional/x\" }"), "err_unwrap");
    assert_eq!(class_of("index out of bounds: the len is 3 but the index is 7"), "index_out_of_bounds");
    assert_eq!(class_of("update state: PoisonError { .. }"), "poisoned");
    assert_eq!(class_of("something with /Users/Fictional/secret in it"), "other");
    // A dependency's place is shortened to the crate and its file.
    assert_eq!(short_location("/Users/runner/.cargo/registry/src/index.crates.io-abc/tao-0.35.0/src/platform_impl/macos/observer.rs:812:9"), "tao-0.35.0/src/platform_impl/macos/observer.rs:812:9");
    assert_eq!(short_location("crates/ceh-desktop/src/main.rs:10:5"), "crates/ceh-desktop/src/main.rs:10:5");

    // A fresh installation: no data folder yet, the hook makes it.
    let root = tempfile::tempdir().unwrap();
    // Nothing happened before: nothing to report.
    assert_eq!(report_previous(root.path(), "/Users/Fictional", at()), None);
    install(root.path());
    let panicked = std::thread::Builder::new().name("ceh-test-worker".into()).spawn(|| {
        let planted = "SYNTHETIC-NIE-Y1234567X";
        let _ = std::panic::catch_unwind(|| panic!("the profile {planted} could not be read"));
    });
    panicked.unwrap().join().unwrap();

    let id = report_previous(root.path(), "/Users/Fictional", at()).expect("the panic was kept for the next start");
    assert_eq!(latest_id(root.path(), "/Users/Fictional").as_deref(), Some(id.as_str()));
    let text = clipboard_text(root.path(), "/Users/Fictional", &id).unwrap();
    assert!(text.contains("crash_note_contract.rs"), "the place of the panic is in the report: {text}");
    assert!(text.contains("ceh-test-worker") && text.contains("previous_panic"), "{text}");
    assert!(!text.contains("SYNTHETIC-NIE") && !text.contains("Y1234567X") && !text.contains("could not be read"), "the message of the panic stays out: {text}");
    // The note is used up: the next start reports nothing again.
    assert_eq!(report_previous(root.path(), "/Users/Fictional", at()), None);
}

#[test]
fn a_note_is_kept_until_its_report_is_saved_and_an_unreadable_one_is_dropped() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let note = data.join("last-panic.json");
    std::fs::write(&note, r#"{"version":"0.4.5","thread":"main","location":"src/lib.rs:1:1","class":"other"}"#).unwrap();
    // The report folder cannot be made (a file is in its place): the only note must survive for the next start.
    std::fs::write(data.join("startup-errors"), "in the way").unwrap();
    assert_eq!(report_previous(root.path(), "/Users/Fictional", at()), None);
    assert!(note.exists(), "the note stays while no report could be saved");
    std::fs::remove_file(data.join("startup-errors")).unwrap();
    assert!(report_previous(root.path(), "/Users/Fictional", at()).is_some(), "the next start reports it");
    assert!(!note.exists(), "and uses it up");
    // A note that is not JSON would never become a report: it is removed.
    std::fs::write(&note, "not json").unwrap();
    assert_eq!(report_previous(root.path(), "/Users/Fictional", at()), None);
    assert!(!note.exists());
}
