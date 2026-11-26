//! Learning mode (developer only): the structure of a page of the appointment site, read without anything a
//! person has typed. `parse_page` is the strict gate: an unknown key (a `value`, say) or a page over the limits
//! is refused, so a script that starts to return more than the structure cannot reach a file.
use crate::bidi::BidiConnection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const SCRIPT: &str = include_str!("learning_snapshot.js");
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
    pub title: String,
    pub headings: Vec<String>,
    pub text: Vec<String>,
    pub forms: Vec<LearningForm>,
    pub controls: Vec<LearningControl>,
}
impl LearningPage {
    /// Changes when the structure or an option list changes (a select filled after another choice).
    pub fn fingerprint(&self) -> String {
        let canonical = serde_json::to_string(self).unwrap_or_default();
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
/// Run the page script in the document's realm and parse what it returns. Read only.
pub fn read_page(connection: &mut BidiConnection, realm: &str) -> Option<LearningPage> {
    let value = connection
        .call_before(
            "script.callFunction",
            serde_json::json!({
                "target": {"realm": realm},
                "functionDeclaration": SCRIPT,
                "awaitPromise": false
            }),
            Instant::now() + Duration::from_millis(750),
        )
        .ok()?;
    parse_page(value["result"]["value"].as_str()?)
}
