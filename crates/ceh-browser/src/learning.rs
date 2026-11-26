//! Learning mode (developer only): the structure of a page of the appointment site, read without anything a
//! person has typed. `parse_page` is the strict gate: an unknown key (a `value`, say) or a page over the limits
//! is refused, so a script that starts to return more than the structure cannot reach a file.
use crate::bidi::BidiConnection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const SCRIPT: &str = include_str!("learning_snapshot.js");
/// Long on purpose: see `ReadFailure::Call`.
const READ_DEADLINE: Duration = Duration::from_secs(20);
const MAX_BYTES: usize = 256 * 1024;
const MAX_CONTROLS: usize = 400;
const MAX_OPTIONS: usize = 500;
const MAX_TEXT: usize = 100;
const MAX_HEADINGS: usize = 50;
const MAX_FORMS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningControl {
    pub tag: String,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub label: String,
    pub form: String,
    pub disabled: bool,
    pub options: Vec<LearningOption>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningForm {
    pub id: String,
    pub name: String,
    pub action: String,
    pub method: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningPage {
    pub path: String,
    /// The load stamp of the document (`performance.timeOrigin`): a refresh is a new document with the same
    /// structure. Not part of the fingerprint.
    pub load: String,
    pub title: String,
    pub headings: Vec<String>,
    pub text: Vec<String>,
    pub forms: Vec<LearningForm>,
    pub controls: Vec<LearningControl>,
}
impl LearningPage {
    /// Changes when the structure or an option list changes (a select filled after another choice).
    pub fn fingerprint(&self) -> String {
        let mut structure = self.clone();
        structure.load.clear();
        let canonical = serde_json::to_string(&structure).unwrap_or_default();
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }
}
/// `None` for malformed JSON, an unknown key, or a page over the limits.
pub fn parse_page(raw: &str) -> Option<LearningPage> {
    if raw.len() > MAX_BYTES {
        return None;
    }
    let page: LearningPage = serde_json::from_str(raw).ok()?;
    let within = page.controls.len() <= MAX_CONTROLS
        && page.controls.iter().all(|c| c.options.len() <= MAX_OPTIONS)
        && page.text.len() <= MAX_TEXT
        && page.headings.len() <= MAX_HEADINGS
        && page.forms.len() <= MAX_FORMS;
    within.then_some(page)
}
/// Why a read gave no page (kept apart so a recording that stops can say why).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFailure {
    /// The browser did not answer in time or the connection is gone. A missed deadline ends the connection for
    /// the whole search, so the deadline is long: a page that is being left can hold the call for seconds.
    Call,
    /// The script declined to read (not the official site) or returned no text.
    Empty,
    /// The text is not the structure: unknown key, over a limit, or not JSON.
    Invalid,
}
/// Run the page script in `target` (`{"context": .., "sandbox": ..}` or a realm) and parse what it returns.
/// Read only.
pub fn read_page(connection: &mut BidiConnection, target: &serde_json::Value) -> Result<LearningPage, ReadFailure> {
    let value = connection
        .call_before(
            "script.callFunction",
            serde_json::json!({
                "target": target,
                "functionDeclaration": SCRIPT,
                "awaitPromise": false
            }),
            Instant::now() + READ_DEADLINE,
        )
        .map_err(|_| ReadFailure::Call)?;
    let text = value["result"]["value"].as_str().ok_or(ReadFailure::Empty)?;
    parse_page(text).ok_or(ReadFailure::Invalid)
}
