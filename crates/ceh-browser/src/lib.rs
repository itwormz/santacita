//! Browser boundary and deterministic replay; no browser engine dependency.
#[cfg(feature = "bidi")]
pub mod applicant;
#[cfg(feature = "bidi")]
pub mod auth;
#[cfg(feature = "bidi")]
pub mod bidi;
#[cfg(feature = "bidi")]
pub mod contact;
mod dispatch;
#[cfg(feature = "bidi")]
pub mod dom;
#[cfg(feature = "bidi")]
pub mod exit;
#[cfg(feature = "bidi")]
mod mutation;
pub mod navigation;
#[cfg(feature = "bidi")]
pub mod observer;
#[cfg(feature = "bidi")]
pub mod passive;
#[cfg(feature = "bidi")]
pub mod evidence;
pub mod recovery_navigation;
pub mod refresh;
#[cfg(feature = "bidi")]
pub mod request;
#[cfg(feature = "bidi")]
pub mod selection;
#[cfg(feature = "bidi")]
mod trusted_input;
#[cfg(feature = "bidi")]
pub use trusted_input::{take_scroll_fallback, take_stale_reason};
#[cfg(feature = "bidi")]
mod typed_field;
#[cfg(test)]
thread_local! {
    /// Tests only: a local test server that page scripts accept as the site, so a whole field entry runs on a
    /// fixture page. Release and debug builds of the app never compile this.
    pub(crate) static TEST_SITE_HOST: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
/// A page script as it is sent. Outside tests it is the script itself; in tests the site's host may be a local server.
pub(crate) fn site_script(script: &str) -> std::borrow::Cow<'_, str> {
    #[cfg(test)]
    if let Some(host) = TEST_SITE_HOST.with(|h| h.borrow().clone()) {
        return std::borrow::Cow::Owned(
            script
                .replace("'https://icp.administracionelectronica.gob.es'", &format!("'http://{host}'"))
                .replace("location.hostname==='icp.administracionelectronica.gob.es'", &format!("location.host==='{host}'")),
        );
    }
    std::borrow::Cow::Borrowed(script)
}
pub use typed_field::take_preparation_reason;
#[cfg(feature = "bidi")]
mod typed_select;
use ceh_core::control::{Authority, ControlError, ControlMode, ControlSnapshot};
use ceh_core::model::{ActionKind, DocumentGuard, Observation, Ownership, SessionId};
pub use dispatch::{ActionPermit, DispatchGate};
use std::{collections::VecDeque, future::Future};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserError {
    NoObservation,
    Paused,
    StaleCommand,
    DuplicateCommand,
    InvalidControl,
    EndOfReplay,
    WrongSession,
    StaleDocument,
    ManualOwnership,
    Unsupported,
}

impl BrowserError {
    /// A fixed name for the report: which kind of error ended an action, never page or user data.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoObservation => "no_observation",
            Self::Paused => "paused",
            Self::StaleCommand => "stale_command",
            Self::DuplicateCommand => "duplicate_command",
            Self::InvalidControl => "invalid_control",
            Self::EndOfReplay => "end_of_replay",
            Self::WrongSession => "wrong_session",
            Self::StaleDocument => "stale_document",
            Self::ManualOwnership => "manual_ownership",
            Self::Unsupported => "unsupported",
        }
    }
}

pub use ceh_core::model::DispatchOutcome;

pub trait BrowserPort {
    /// Apply trusted control before dequeuing further browser commands.
    fn apply_control(&mut self, snapshot: &ControlSnapshot) -> Result<(), BrowserError>;
    fn observe(&mut self) -> impl Future<Output = Result<Observation, BrowserError>> + Send;
    /// Recheck session, document, recognized stage and ownership immediately before dispatch.
    fn execute(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
    ) -> impl Future<Output = Result<DispatchOutcome, BrowserError>> + Send;
    fn handoff(
        &mut self,
        guard: &DocumentGuard,
    ) -> impl Future<Output = Result<(), BrowserError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchedAction {
    pub guard: DocumentGuard,
    pub action: ActionKind,
}

/// Advances only through supplied observations; never creates a browser or I/O.
pub struct ReplayBrowser {
    session: SessionId,
    pending: VecDeque<Observation>,
    current: Option<Observation>,
    authority: Authority,
    actions: Vec<DispatchedAction>,
}

impl ReplayBrowser {
    pub fn new(session: SessionId, observations: Vec<Observation>) -> Self {
        Self {
            authority: Authority::new(session.clone()),
            session,
            pending: observations.into(),
            current: None,
            actions: Vec::new(),
        }
    }
    pub fn actions(&self) -> &[DispatchedAction] {
        &self.actions
    }
    pub fn ownership(&self) -> Ownership {
        if self.authority.mode() == ControlMode::Automatic {
            Ownership::Automatic
        } else {
            Ownership::Manual
        }
    }

    fn validate(&self, guard: &DocumentGuard) -> Result<(), BrowserError> {
        if self.session != guard.session {
            return Err(BrowserError::WrongSession);
        }
        if guard.epoch != self.authority.epoch() {
            return Err(BrowserError::StaleCommand);
        }
        let current = self.current.as_ref().ok_or(BrowserError::NoObservation)?;
        if current.document != guard.document || current.kind != guard.expected_page {
            return Err(BrowserError::StaleDocument);
        }
        Ok(())
    }
}

impl BrowserPort for ReplayBrowser {
    fn apply_control(&mut self, snapshot: &ControlSnapshot) -> Result<(), BrowserError> {
        self.authority.apply(snapshot).map_err(|error| match error {
            ControlError::WrongSession => BrowserError::WrongSession,
            ControlError::ManualOwnership => BrowserError::ManualOwnership,
            ControlError::StaleUpdate => BrowserError::StaleCommand,
            ControlError::EpochExhausted => BrowserError::InvalidControl,
        })
    }
    async fn observe(&mut self) -> Result<Observation, BrowserError> {
        if self.authority.mode() == ControlMode::Paused {
            return Err(BrowserError::Paused);
        }
        let next = self.pending.pop_front().ok_or(BrowserError::EndOfReplay)?;
        self.current = Some(next.clone());
        Ok(next)
    }
    async fn execute(
        &mut self,
        guard: &DocumentGuard,
        action: ActionKind,
    ) -> Result<DispatchOutcome, BrowserError> {
        if self.authority.mode() == ControlMode::Paused {
            return Err(BrowserError::Paused);
        }
        if self.authority.mode() == ControlMode::Manual {
            return Err(BrowserError::ManualOwnership);
        }
        self.validate(guard)?;
        if self
            .actions
            .iter()
            .any(|previous| previous.guard.document == guard.document && previous.action == action)
        {
            return Err(BrowserError::DuplicateCommand);
        }
        self.actions.push(DispatchedAction {
            guard: guard.clone(),
            action,
        });
        Ok(DispatchOutcome::Dispatched)
    }
    async fn handoff(&mut self, guard: &DocumentGuard) -> Result<(), BrowserError> {
        self.validate(guard)?;
        self.authority
            .handoff()
            .map_err(|_| BrowserError::InvalidControl)?;
        Ok(())
    }
}

#[cfg(feature = "bidi")]
pub mod cookies;

#[cfg(feature = "bidi")]
pub mod discovery;

#[cfg(feature = "bidi")]
pub mod icp_journal;

#[cfg(feature = "bidi")]
pub mod public_details;

#[cfg(feature = "bidi")]
pub mod recovery_controls;

#[cfg(feature = "bidi")]
pub mod launch;

#[cfg(feature = "bidi")]
mod spawn;

#[cfg(feature = "bidi")]
pub mod firefox;
#[cfg(feature = "bidi")]
pub mod firefox_control;

#[cfg(feature = "bidi")]
pub mod chromium;

#[cfg(feature = "bidi")]
mod cdp;

#[cfg(feature = "bidi")]
pub mod session_driver;
pub mod report_state;

#[cfg(feature = "bidi")]
mod firefox_certificates;
#[cfg(feature = "bidi")]
pub use firefox_certificates::{register_helper, run_helper_if_requested};

/// For the opt-in live tests: a browser that is not installed is reported as `SKIPPED` (never as a silent pass),
/// and fails the test when `CEH_REQUIRE_BROWSERS` is set (a machine that is meant to have the browsers).
#[doc(hidden)]
pub fn skip_missing_browser(name: &str) {
    assert!(std::env::var_os("CEH_REQUIRE_BROWSERS").is_none() && std::env::var_os("CEH_REQUIRE_BROWSER").is_none(), "{name} is required but not installed");
    eprintln!("SKIPPED: {name} not installed");
}

#[cfg(test)]
mod browser_error_code_tests {
    use super::BrowserError::{self, *};

    #[test]
    fn every_browser_error_has_a_fixed_code_for_the_report() {
        let all = [
            (NoObservation, "no_observation"),
            (Paused, "paused"),
            (StaleCommand, "stale_command"),
            (DuplicateCommand, "duplicate_command"),
            (InvalidControl, "invalid_control"),
            (EndOfReplay, "end_of_replay"),
            (WrongSession, "wrong_session"),
            (StaleDocument, "stale_document"),
            (ManualOwnership, "manual_ownership"),
            (Unsupported, "unsupported"),
        ];
        for (error, code) in all {
            assert_eq!(BrowserError::code(&error), code);
            assert!(code.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{code} is a plain name");
        }
    }
}
