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
    sync::LazyLock,
    time::{Duration, Instant},
};

const MAX_PAGES: u32 = 512;
const READ_EVERY: Duration = Duration::from_millis(1500);
static EMAIL: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap());
static DNI: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)\b\d{8}[A-Z]\b").unwrap());
static NIE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)\b[XYZ][\s\-]?\d{7}[\s\-]?[A-Z]\b").unwrap());
static PHONE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\+?\d(?:[\s.\-]?\d){8,}").unwrap());

struct Last {
    index: u32,
    load: String,
    fingerprint: String,
    path: String,
    form_actions: Vec<String>,
}
pub struct LearningRecorder {
    directory: PathBuf,
    sanitizer: Sanitizer,
    /// Folded (lower case, no accents) strings of the profile that are blanked in any spelling.
    known: Vec<Vec<char>>,
    index: u32,
    last: Option<Last>,
    stopped: bool,
    read_at: Option<Instant>,
    /// The last failure written to `events.jsonl`; cleared when a page is read again.
    noted: Option<String>,
}
/// Lower case without Spanish accents, one char for one char, so positions line up with the original text.
fn fold(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| match c.to_lowercase().next().unwrap_or(c) {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}
fn blank_known(text: &str, known: &[Vec<char>]) -> String {
    let original: Vec<char> = text.chars().collect();
    let folded = fold(text);
    let mut hidden = vec![false; original.len()];
    for needle in known {
        if needle.is_empty() || needle.len() > folded.len() {
            continue;
        }
        for start in 0..=folded.len() - needle.len() {
            if folded[start..start + needle.len()] == needle[..] {
                hidden[start..start + needle.len()].iter_mut().for_each(|h| *h = true);
            }
        }
    }
    let mut out = String::new();
    let mut previous = false;
    for (c, hide) in original.iter().zip(hidden) {
        if hide {
            if !previous {
                out.push_str("***");
            }
        } else {
            out.push(*c);
        }
        previous = hide;
    }
    out
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
fn scrub(text: &str, sanitizer: &Sanitizer, known: &[Vec<char>]) -> String {
    let mut text = blank_known(&sanitizer.sanitize(text), known);
    for pattern in [&*EMAIL, &*NIE, &*DNI, &*PHONE] {
        text = pattern.replace_all(&text, "***").into_owned();
    }
    text
}
fn redact(value: &mut Value, sanitizer: &Sanitizer, known: &[Vec<char>]) {
    match value {
        Value::String(text) => *text = scrub(text, sanitizer, known),
        Value::Array(items) => items.iter_mut().for_each(|item| redact(item, sanitizer, known)),
        Value::Object(map) => map.values_mut().for_each(|item| redact(item, sanitizer, known)),
        _ => {}
    }
}
/// Where the recording should go, as asked from the developer window (`Some(Some(root))` on, `Some(None)` off);
/// `None` until asked, when `CEH_LEARNING` decides.
static REQUESTED: std::sync::Mutex<Option<Option<std::path::PathBuf>>> = std::sync::Mutex::new(None);
/// Turn the recording on (into `root`) or off for the running search and the next ones. A search picks it up at its next pass.
pub fn request(root: Option<std::path::PathBuf>) {
    if let Ok(mut requested) = REQUESTED.lock() {
        *requested = Some(root);
    }
}
/// The folder the recording goes to now: the developer window's choice, or `CEH_LEARNING` when nothing was chosen.
pub fn requested() -> Option<std::path::PathBuf> {
    let asked = REQUESTED.lock().ok().and_then(|requested| requested.clone());
    match asked {
        Some(choice) => choice,
        None => std::env::var_os("CEH_LEARNING").filter(|value| !value.is_empty()).map(std::path::PathBuf::from),
    }
}
impl LearningRecorder {
    /// `None` unless `CEH_LEARNING` names an output directory. Says where it records (or why not) on stderr.
    pub fn from_env(profile: &ProfileDraft) -> Option<Self> {
        let root = std::env::var_os("CEH_LEARNING").filter(|value| !value.is_empty())?;
        Self::start(Path::new(&root), profile)
    }
    /// A new run directory under `root`, named after the Madrid time of the start.
    pub fn start(root: &Path, profile: &ProfileDraft) -> Option<Self> {
        let mut label = crate::session_timeline::madrid_text(None).replace(':', "-");
        if label.is_empty() {
            label = "run".into();
        }
        let made = Self::create(root, &label, profile);
        match &made {
            Some(recorder) => eprintln!("learning mode: recording to {}", recorder.directory.display()),
            None => eprintln!("learning mode: cannot create a directory under {} (use an absolute path)", root.display()),
        }
        made
    }
    /// The run's own directory `<root>/<label>` (`<label>-2`, ... when it exists), private to the user.
    pub fn create(root: &Path, label: &str, profile: &ProfileDraft) -> Option<Self> {
        private_directory(root).ok()?;
        let directory = (1..100)
            .map(|n| if n == 1 { root.join(label) } else { root.join(format!("{label}-{n}")) })
            .find(|candidate| !candidate.exists())?;
        private_directory(&directory).ok()?;
        let mut sanitizer = Sanitizer::default();
        let secrets = [profile.document_number.clone(), profile.full_name.clone(), profile.phone.clone(), profile.email.clone()];
        sanitizer.register(secrets.clone());
        let mut known: Vec<Vec<char>> = secrets.iter().map(|secret| fold(secret)).filter(|f| f.len() >= 3).collect();
        known.extend(profile.full_name.split_whitespace().filter(|word| word.chars().count() >= 4).map(fold));
        Some(Self { directory, sanitizer, known, index: 0, last: None, stopped: false, read_at: None, noted: None })
    }
    /// Write a failure of the recording itself to `events.jsonl` (once per kind until a page is read again), so a
    /// recording that stops can say why. Also printed to stderr the first time.
    pub fn note(&mut self, event: &str) {
        if self.stopped || self.noted.as_deref() == Some(event) {
            return;
        }
        self.noted = Some(event.to_owned());
        let at = crate::current_time().map(|now| now.to_rfc3339()).unwrap_or_default();
        eprintln!("learning mode: {event}");
        let _ = private_file(true)
            .open(self.directory.join("events.jsonl"))
            .and_then(|mut f| writeln!(f, "{}", json!({"at": at, "event": event})));
    }
    /// Whether a read of the page is due: at most one every 1.5 seconds, so the extra script call stays rare.
    pub fn due(&mut self, now: Instant) -> bool {
        if self.stopped || self.read_at.is_some_and(|at| now.saturating_duration_since(at) < READ_EVERY) {
            return false;
        }
        self.read_at = Some(now);
        true
    }
    /// Write the page when its structure or its document differs from the last one written (a refresh is a new
    /// document with the same structure and is written too), and the transition from the last page to it.
    pub fn record(&mut self, page: LearningPage) {
        if self.stopped || self.index >= MAX_PAGES {
            return;
        }
        self.noted = None;
        // The load stamp is a number of digits the phone pattern would blank: compare and keep it as read.
        let load = page.load.clone();
        let mut value = serde_json::to_value(&page).unwrap_or(Value::Null);
        redact(&mut value, &self.sanitizer, &self.known);
        let Ok(mut page) = serde_json::from_value::<LearningPage>(value) else { return };
        page.load = load;
        let fingerprint = page.fingerprint();
        if self.last.as_ref().is_some_and(|last| last.load == page.load && last.fingerprint == fingerprint) {
            return;
        }
        let index = self.index + 1;
        let at = crate::current_time().map(|now| now.to_rfc3339()).unwrap_or_default();
        let file = json!({"index": index, "at": at, "page": page});
        let transition = json!({
            "from": self.last.as_ref().map(|last| last.index), "to": index,
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
            load: page.load.clone(),
            fingerprint,
            path: page.path.clone(),
            form_actions: page.forms.iter().map(|form| form.action.clone()).collect(),
        });
    }
}
