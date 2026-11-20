//! The place of a panic, kept for the next start. A panic on the window's own thread aborts the whole app and
//! leaves no message a person could read, so the hook writes a small note first: where in the code, on which
//! thread, and a fixed class of the message. The message itself is never kept (it may carry data).
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static NOTE: OnceLock<PathBuf> = OnceLock::new();

fn note_path(root: &Path) -> PathBuf {
    root.join("data/last-panic.json")
}
/// The kind of a panic message, from the fixed phrases of the standard library; anything else is `other`.
pub fn class_of(message: &str) -> &'static str {
    const KNOWN: [(&str, &str); 7] = [
        ("called `Option::unwrap()` on a `None` value", "none_unwrap"),
        ("called `Result::unwrap()` on an `Err` value", "err_unwrap"),
        ("index out of bounds", "index_out_of_bounds"),
        ("attempt to ", "arithmetic_overflow"),
        ("already borrowed", "already_borrowed"),
        ("already mutably borrowed", "already_borrowed"),
        ("PoisonError", "poisoned"),
    ];
    for (start, class) in KNOWN {
        if message.starts_with(start) {
            return class;
        }
    }
    if message.contains("PoisonError") {
        return "poisoned";
    }
    "other"
}
/// A dependency's place without the build machine's path: `tao-0.35.0/src/...`.
pub fn short_location(location: &str) -> String {
    match location.split_once("/registry/src/") {
        Some((_, after)) => after.split_once('/').map_or(after, |(_, rest)| rest).to_owned(),
        None => location.to_owned(),
    }
}
/// Keep the place of every panic from now on in `data/last-panic.json` (the last one wins).
pub fn install(root: &Path) {
    let _ = NOTE.set(note_path(root));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = NOTE.get() {
            let location = info.location().map(|l| short_location(&format!("{}:{}:{}", l.file(), l.line(), l.column()))).unwrap_or_default();
            let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
            let note = json!({
                "version": env!("CARGO_PKG_VERSION"),
                "thread": thread,
                "location": location,
                "class": class_of(info.payload_as_str().unwrap_or("")),
            });
            let temporary = path.with_extension("tmp");
            if std::fs::write(&temporary, note.to_string()).is_ok() {
                let _ = std::fs::rename(&temporary, path);
            }
        }
        previous(info);
    }));
}
/// At the next start: turn the kept note into a report the About window can copy. `None` when there is no note.
pub fn report_previous(root: &Path, home: &str, now: chrono::DateTime<chrono::Utc>) -> Option<String> {
    let path = note_path(root);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let note: Value = serde_json::from_str(&text).ok()?;
    let field = |name: &str| note[name].as_str().map(|v| v.chars().take(200).collect::<String>()).unwrap_or_default();
    let summary = format!("{} in {} ({})", field("location"), field("thread"), field("class"));
    let report = json!({
        "version": field("version"),
        "time": now.to_rfc3339(),
        "system": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "frozen": false,
        "executable": "",
        "phase": "previous_panic",
        "bundle_checks": {},
        "error_type": "panic",
        "message": "the app stopped on a panic",
        "traceback": [],
        "session": {"stop": {"reason": "panic", "summary": summary}},
    });
    crate::startup_report::save_session_stop(root, home, report, now)
}
