//! Learning mode (developer only, `CEH_LEARNING=<directory>`): while the owner drives the app's browser by hand,
//! every page's structure and every page-to-page transition is written to `<directory>/<run>/`. Only the
//! structure of the page is recorded (see `ceh_browser::learning`); what the profile holds, and anything shaped
//! like a NIE, is replaced with `***` in the text before it is written. Any write failure ends the recording
//! silently: it never gets in the way of the search.
use ceh_browser::learning::LearningPage;
use ceh_core::profiles::ProfileDraft;
use ceh_services::diagnostics::Sanitizer;
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_PAGES: u32 = 512;

struct Last {
    index: u32,
    document: String,
    fingerprint: String,
    path: String,
    form_actions: Vec<String>,
}
pub struct LearningRecorder {
    directory: PathBuf,
    sanitizer: Sanitizer,
    index: u32,
    last: Option<Last>,
    stopped: bool,
}
fn private_directory(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}
fn private_file(append: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    if append {
        options.create(true).append(true);
    } else {
        options.create_new(true).write(true);
    }
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}
fn redact(value: &mut Value, sanitizer: &Sanitizer) {
    match value {
        Value::String(text) => *text = sanitizer.sanitize(text),
        Value::Array(items) => items.iter_mut().for_each(|item| redact(item, sanitizer)),
        Value::Object(map) => map.values_mut().for_each(|item| redact(item, sanitizer)),
        _ => {}
    }
}
/// The snake-case name a serialisable enum value has in the files (page kind, request method).
pub fn name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}
impl LearningRecorder {
    /// `None` unless `CEH_LEARNING` names an output directory.
    pub fn from_env(profile: &ProfileDraft) -> Option<Self> {
        let root = std::env::var_os("CEH_LEARNING").filter(|value| !value.is_empty())?;
        let label = crate::session_timeline::madrid_text(None).replace(':', "-");
        Self::create(Path::new(&root), &label, profile)
    }
    /// The run's own directory `<root>/<label>`, private to the user.
    pub fn create(root: &Path, label: &str, profile: &ProfileDraft) -> Option<Self> {
        let directory = root.join(label);
        private_directory(&directory).ok()?;
        let mut sanitizer = Sanitizer::default();
        sanitizer.register([
            profile.document_number.clone(), profile.full_name.clone(), profile.phone.clone(), profile.email.clone(),
        ]);
        Some(Self { directory, sanitizer, index: 0, last: None, stopped: false })
    }
    /// Write the page when its structure or its document differs from the last one written (a refresh is a new
    /// document with the same structure and is written too), and the transition from the last page to it.
    pub fn record(&mut self, page: LearningPage, method: &str, kind: &str, document: &str) {
        if self.stopped || self.index >= MAX_PAGES {
            return;
        }
        let mut value = serde_json::to_value(&page).unwrap_or(Value::Null);
        redact(&mut value, &self.sanitizer);
        let Ok(page) = serde_json::from_value::<LearningPage>(value) else { return };
        let fingerprint = page.fingerprint();
        if self.last.as_ref().is_some_and(|last| last.document == document && last.fingerprint == fingerprint) {
            return;
        }
        let index = self.index + 1;
        let at = crate::current_time().map(|now| now.to_rfc3339()).unwrap_or_default();
        let file = json!({"index": index, "at": at, "kind": kind, "method": method, "page": page});
        let transition = json!({
            "from": self.last.as_ref().map(|last| last.index), "to": index, "method": method,
            "via_forms": self.last.as_ref().map(|last| last.form_actions.clone()).unwrap_or_default(),
            "from_path": self.last.as_ref().map(|last| last.path.clone()), "to_path": page.path,
        });
        let written = private_file(false)
            .open(self.directory.join(format!("{index:04}.json")))
            .and_then(|mut f| f.write_all(file.to_string().as_bytes()))
            .and_then(|()| private_file(true).open(self.directory.join("path.jsonl")))
            .and_then(|mut f| writeln!(f, "{transition}"));
        if written.is_err() {
            self.stopped = true;
            return;
        }
        self.index = index;
        self.last = Some(Last {
            index,
            document: document.to_owned(),
            fingerprint,
            path: page.path.clone(),
            form_actions: page.forms.iter().map(|form| form.action.clone()).collect(),
        });
    }
}
